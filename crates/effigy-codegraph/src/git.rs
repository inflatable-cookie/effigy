//! Git provenance and stamps recorded by explicit indexing.
//!
//! Every git-less fallback is graceful: any failure (no `.git`, missing `git`
//! binary, unborn HEAD, dirty tree, non-UTF-8 output) yields unknown evidence;
//! freshness reads use the scan-state walk and never refresh the index.

use std::path::Path;
use std::process::Command;

use crate::error::CodeGraphError;
use crate::scope::GraphScope;
use crate::storage::GraphStore;

/// Metadata key recording the git HEAD the index was built from.
///
/// The stamp is written only when the working tree was clean at index time;
/// an index built over uncommitted edits carries no stamp, so the gate can
/// never mistake a dirty-tree snapshot for the committed tree. The stamp is
/// per scope, so a scope that was never indexed cannot borrow another
/// scope's freshness.
pub(crate) const GIT_INDEXED_HEAD_KEY: &str = "git_indexed_head";

fn git_indexed_head_key(scope_key: &str) -> String {
    if scope_key.is_empty() {
        GIT_INDEXED_HEAD_KEY.to_owned()
    } else {
        format!("{GIT_INDEXED_HEAD_KEY}:{scope_key}")
    }
}

/// HEAD the index of one scope was built from, or `None` when that scope has
/// no clean-tree stamp. Absent means "unknown", never "current".
pub(crate) fn indexed_head_for_scope(
    store: &GraphStore,
    scope_key: &str,
) -> Result<Option<String>, CodeGraphError> {
    store.metadata_value(&git_indexed_head_key(scope_key))
}

/// Current `HEAD` of `repo_root`, or `None` when git is unavailable, the repo
/// has no commits, or HEAD cannot be resolved.
pub(crate) fn current_head(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let head = String::from_utf8(output.stdout).ok()?;
    let head = head.trim();
    if head.is_empty() {
        None
    } else {
        Some(head.to_owned())
    }
}

/// Whether the checkout is clean apart from graph-skipped output paths.
pub(crate) fn working_tree_clean_for_scope(scope: &GraphScope) -> bool {
    porcelain_status_paths(scope.workspace_root(), &[])
        .map(|paths| {
            paths
                .iter()
                .all(|path| porcelain_entry_is_walk_skipped(path))
        })
        .unwrap_or(false)
}

/// `git status --porcelain` for the given pathspecs, or `None` when git cannot
/// answer. An empty pathspec list asks git for the whole worktree.
fn porcelain_status_paths(repo_root: &Path, pathspecs: &[String]) -> Option<Vec<String>> {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("status")
        .arg("--porcelain")
        .current_dir(repo_root);
    if !pathspecs.is_empty() {
        command.arg("--");
        for pathspec in pathspecs {
            command.arg(pathspec);
        }
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    Some(stdout.lines().map(str::to_owned).collect())
}

/// A porcelain line is irrelevant to graph freshness when its path is one the
/// graph walk skips entirely (`XY path`, where `XY` is the two status chars).
fn porcelain_entry_is_walk_skipped(line: &str) -> bool {
    let Some(path) = line.get(3..) else {
        return false;
    };
    crate::walk::should_skip_path(path.trim())
}

pub(crate) fn clear_index_stamp(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<(), CodeGraphError> {
    store.delete_metadata(&git_indexed_head_key(scope.key()))
}

/// Record (or clear) the git stamp for one scope after an index build.
pub(crate) fn update_index_stamp(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<(), CodeGraphError> {
    let key = git_indexed_head_key(scope.key());
    match (
        current_head(scope.workspace_root()),
        working_tree_clean_for_scope(scope),
    ) {
        (Some(head), true) => store.save_metadata(&key, &head),
        _ => store.delete_metadata(&key),
    }
}

/// Repository-relative paths with any working-tree change (tracked or
/// untracked), or `None` when git cannot answer.
///
/// `None` is not "clean": callers treat it as "identity unknown" and must not
/// label any excerpt as committed bytes.
pub(crate) fn dirty_paths(repo_root: &Path) -> Option<std::collections::BTreeSet<String>> {
    let mut command = Command::new("git");
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("status")
        .args(["--porcelain=v1", "-z", "--untracked-files=all"])
        .current_dir(repo_root);
    #[cfg(debug_assertions)]
    if std::env::var_os("EFFIGY_TEST_DOCTOR_GIT_EVIDENCE").is_some() {
        command
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_CALLER",
                "effigy-codegraph::git::dirty_paths",
            )
            .env(
                "EFFIGY_TEST_DOCTOR_GIT_ARGS_JSON",
                serde_json::json!(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
                    .to_string(),
            );
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    let mut paths = std::collections::BTreeSet::new();
    if output.stdout.is_empty() {
        return Some(paths);
    }
    let records = output.stdout.strip_suffix(&[0])?;
    let mut records = records.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        if record.len() < 4 || record[2] != b' ' {
            return None;
        }
        paths.insert(String::from_utf8(record[3..].to_vec()).ok()?);
        // In porcelain v1 -z, a rename/copy has two NUL-terminated paths:
        // destination first, then source. Both identities are uncertain.
        if matches!(record[0], b'R' | b'C') || matches!(record[1], b'R' | b'C') {
            paths.insert(String::from_utf8(records.next()?.to_vec()).ok()?);
        }
    }
    Some(paths)
}

/// Positive evidence that an exact repository-relative file has HEAD's bytes.
pub(crate) fn path_at_head(repo_root: &Path, path: &str) -> bool {
    let Ok(output) = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("show")
        .arg(format!("HEAD:{path}"))
        .current_dir(repo_root)
        .output()
    else {
        return false;
    };
    output.status.success()
        && std::fs::read(repo_root.join(path))
            .map(|bytes| bytes == output.stdout)
            .unwrap_or(false)
}

#[cfg(test)]
mod dirty_paths_tests {
    use super::*;

    fn git(root: &Path, args: &[&str]) {
        let output = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(output.status.success(), "{output:?}");
    }

    #[test]
    fn provenance_status_does_not_refresh_the_git_index() {
        let temp = tempfile::tempdir().expect("owned Git fixture");
        let root = temp.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "fixture@example.invalid"]);
        git(root, &["config", "user.name", "Fixture"]);
        git(root, &["config", "core.fsmonitor", "false"]);
        let tracked = root.join("clean.md");
        std::fs::write(&tracked, "same bytes\n").unwrap();
        git(root, &["add", "clean.md"]);
        git(root, &["commit", "-qm", "fixture"]);
        let index = root.join(".git/index");
        let first = std::fs::read(&index).unwrap();
        let file = std::fs::File::open(&tracked).unwrap();
        let now = std::time::SystemTime::now();
        file.set_modified(now - std::time::Duration::from_secs(60))
            .unwrap();
        let control = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["status", "--porcelain"])
            .env("GIT_OPTIONAL_LOCKS", "1")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(control.status.success());
        assert_ne!(
            std::fs::read(&index).unwrap(),
            first,
            "control must refresh cached stat data"
        );
        file.set_modified(now - std::time::Duration::from_secs(120))
            .unwrap();
        let before = std::fs::read(&index).unwrap();
        let before_time = std::fs::metadata(&index).unwrap().modified().unwrap();
        let git_time = std::fs::metadata(root.join(".git"))
            .unwrap()
            .modified()
            .unwrap();
        assert!(dirty_paths(root).unwrap().is_empty());
        assert!(working_tree_clean_for_scope(
            &GraphScope::repo_root(root).unwrap()
        ));
        assert_eq!(std::fs::read(&index).unwrap(), before);
        assert_eq!(
            std::fs::metadata(&index).unwrap().modified().unwrap(),
            before_time
        );
        assert_eq!(
            std::fs::metadata(root.join(".git"))
                .unwrap()
                .modified()
                .unwrap(),
            git_time
        );
    }

    #[test]
    fn nul_status_decodes_quoted_names_and_both_rename_endpoints() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path();
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "fixture@example.invalid"]);
        git(root, &["config", "user.name", "Fixture"]);
        std::fs::write(root.join("before name.md"), "same\n").expect("write");
        std::fs::write(root.join("quo\"té.md"), "clean\n").expect("write");
        std::fs::write(root.join("clean.md"), "clean\n").expect("write");
        git(root, &["add", "-A"]);
        git(root, &["commit", "-qm", "fixture"]);
        git(root, &["mv", "before name.md", "after name.md"]);
        std::fs::write(root.join("quo\"té.md"), "dirty\n").expect("dirty");
        std::fs::write(root.join("untracked space.md"), "new\n").expect("untracked");

        let dirty = dirty_paths(root).expect("git status");
        for name in [
            "before name.md",
            "after name.md",
            "quo\"té.md",
            "untracked space.md",
        ] {
            assert!(dirty.contains(name), "{name}: {dirty:?}");
        }
        assert!(path_at_head(root, "clean.md"));
        assert!(!path_at_head(root, "before name.md"));
        assert!(!path_at_head(root, "after name.md"));
    }
}
