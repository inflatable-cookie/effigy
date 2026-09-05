//! Git-backed freshness fast path for the lazy refresh gate.
//!
//! Every git-less fallback is graceful: any failure (no `.git`, missing `git`
//! binary, unborn HEAD, dirty tree, non-UTF-8 output) simply disables the gate
//! and the caller falls back to the scan-state walk.

use std::path::Path;
use std::process::Command;

use crate::error::CodeGraphError;
use crate::storage::GraphStore;

/// Metadata key recording the git HEAD the index was built from.
///
/// The stamp is written only when the working tree was clean at index time;
/// an index built over uncommitted edits carries no stamp, so the gate can
/// never mistake a dirty-tree snapshot for the committed tree.
pub(crate) const GIT_INDEXED_HEAD_KEY: &str = "git_indexed_head";

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

/// Whether `repo_root` has a clean working tree (no tracked or untracked
/// changes), ignoring paths the graph walk itself skips (`.effigy/`,
/// `target/`, `node_modules/`, `vendor/`, `.git/`). Any failure reports
/// unclean.
pub(crate) fn working_tree_clean(repo_root: &Path) -> bool {
    let output = match Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .current_dir(repo_root)
        .output()
    {
        Ok(output) => output,
        Err(_) => return false,
    };
    if !output.status.success() {
        return false;
    }
    let Ok(stdout) = String::from_utf8(output.stdout) else {
        return false;
    };
    stdout.lines().all(porcelain_entry_is_walk_skipped)
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
    repo_root: &Path,
    store: &GraphStore,
) -> Result<bool, CodeGraphError> {
    let Some(indexed_head) = store.metadata_value(GIT_INDEXED_HEAD_KEY)? else {
        return Ok(false);
    };
    if !working_tree_clean(repo_root) {
        return Ok(false);
    }
    Ok(current_head(repo_root).as_deref() == Some(indexed_head.as_str()))
}

/// Record (or clear) the git stamp after an index build.
pub(crate) fn update_index_stamp(
    repo_root: &Path,
    store: &GraphStore,
) -> Result<(), CodeGraphError> {
    match (current_head(repo_root), working_tree_clean(repo_root)) {
        (Some(head), true) => store.save_metadata(GIT_INDEXED_HEAD_KEY, &head),
        _ => store.delete_metadata(GIT_INDEXED_HEAD_KEY),
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
