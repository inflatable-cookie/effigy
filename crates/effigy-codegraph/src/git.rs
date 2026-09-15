//! Git-backed freshness fast path for the lazy refresh gate.
//!
//! Every git-less fallback is graceful: any failure (no `.git`, missing `git`
//! binary, unborn HEAD, dirty tree, non-UTF-8 output) simply disables the gate
//! and the caller falls back to the scan-state walk.

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

/// Whether the working tree is clean **inside one scope**.
///
/// Git limits the status query to the scope's own pathspec, so a dirty or huge
/// sibling catalog is never walked while checking this scope's freshness. The
/// root scope excludes every segmented descendant; a catalog scope includes
/// only its own root and excludes nested segmented catalogs.
pub(crate) fn working_tree_clean_for_scope(scope: &GraphScope) -> bool {
    let mut pathspecs = Vec::new();
    if scope.is_workspace() {
        // The repository-owned corpus has no prunes, so git reports the whole
        // worktree exactly as it did before catalog scopes existed.
    } else if scope.relative_root().is_empty() {
        pathspecs.push(".".to_owned());
    } else {
        pathspecs.push(scope.relative_root().to_owned());
    }
    for prune in scope.prune_relative_roots() {
        pathspecs.push(format!(":(exclude){prune}"));
    }
    porcelain_status_paths(scope.workspace_root(), &pathspecs)
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

/// Git skip-gate: true only when the stored index stamp exists, the current
/// HEAD matches it, and the working tree is clean — the indexed tree then
/// provably equals the current tree, so the freshness walk can be skipped.
///
/// Conservative by construction: every failure mode returns `false`, which
/// just means "run the scan-state walk" (the behavior before the gate).
pub(crate) fn git_gate_says_fresh(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<bool, CodeGraphError> {
    let Some(indexed_head) = store.metadata_value(&git_indexed_head_key(scope.key()))? else {
        return Ok(false);
    };
    if !working_tree_clean_for_scope(scope) {
        return Ok(false);
    }
    Ok(current_head(scope.workspace_root()).as_deref() == Some(indexed_head.as_str()))
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
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let mut paths = std::collections::BTreeSet::new();
    for line in stdout.lines() {
        // `XY <path>`, or `XY <old> -> <new>` for a rename. An unparsable
        // line makes the whole answer unknown rather than partly wrong.
        let rest = line.get(3..)?;
        for part in rest.split(" -> ") {
            let part = part.trim().trim_matches('"');
            if !part.is_empty() {
                paths.insert(part.to_owned());
            }
        }
    }
    Some(paths)
}
