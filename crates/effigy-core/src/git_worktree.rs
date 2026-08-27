//! Linked git worktree layout resolution.
//!
//! A linked worktree (`git worktree add`) does not own a `.git` directory. It
//! owns a `.git` *file* holding `gitdir: <path>`, pointing back into the
//! primary checkout's `.git/worktrees/<name>`. Two Effigy surfaces care:
//!
//! - container mounts, where that host-absolute pointer means nothing inside
//!   the container unless the shared git directory is visible at the same path
//! - machine-local state that is deliberately not version controlled (the
//!   local secrets vault), which a fresh worktree does not inherit
//!
//! Resolution is pure filesystem reading — no `git` subprocess — so it works
//! inside minimal containers and costs two small reads.

use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// Git directory layout behind a linked worktree checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkedWorktree {
    /// This worktree's private git directory
    /// (`<primary>/.git/worktrees/<name>`), exactly as the `.git` file spells
    /// it so container-side pointers keep resolving.
    pub worktree_git_dir: PathBuf,
    /// The shared git directory every linked worktree points at
    /// (`<primary>/.git`).
    pub common_git_dir: PathBuf,
    /// The primary checkout root, when `common_git_dir` is its `.git` child.
    pub primary_checkout_root: Option<PathBuf>,
}

/// Resolve the git layout behind `repo_root`, or `None` when `repo_root` is a
/// normal checkout, a bare repo, or not a git repository at all.
pub fn detect_linked_worktree(repo_root: &Path) -> Option<LinkedWorktree> {
    // A normal checkout has `.git` as a directory; reading it as a file fails.
    let raw = fs::read_to_string(repo_root.join(".git")).ok()?;
    let pointer = raw
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let pointer = Path::new(pointer);
    let worktree_git_dir = if pointer.is_absolute() {
        lexically_normalize(pointer)
    } else {
        lexically_normalize(&repo_root.join(pointer))
    };
    if !worktree_git_dir.is_dir() {
        return None;
    }
    let common_git_dir = read_common_dir(&worktree_git_dir);
    let primary_checkout_root = (common_git_dir.file_name() == Some(OsStr::new(".git")))
        .then(|| common_git_dir.parent().map(Path::to_path_buf))
        .flatten();
    Some(LinkedWorktree {
        worktree_git_dir,
        common_git_dir,
        primary_checkout_root,
    })
}

/// Resolve `relative` against the primary checkout when `repo_root` is a
/// linked worktree and the worktree's own copy is absent.
///
/// Returns `None` when `repo_root` is not a linked worktree, when the primary
/// checkout cannot be derived, or when the primary copy does not exist either
/// — callers keep their own missing-state error in that case.
pub fn primary_checkout_fallback(repo_root: &Path, relative: &Path) -> Option<PathBuf> {
    if relative.is_absolute() {
        return None;
    }
    let primary = detect_linked_worktree(repo_root)?.primary_checkout_root?;
    if primary == repo_root {
        return None;
    }
    let candidate = primary.join(relative);
    candidate.exists().then_some(candidate)
}

fn read_common_dir(worktree_git_dir: &Path) -> PathBuf {
    let Ok(raw) = fs::read_to_string(worktree_git_dir.join("commondir")) else {
        return worktree_git_dir.to_path_buf();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return worktree_git_dir.to_path_buf();
    }
    let common = Path::new(trimmed);
    if common.is_absolute() {
        lexically_normalize(common)
    } else {
        lexically_normalize(&worktree_git_dir.join(common))
    }
}

/// Collapse `.` and `..` without touching the filesystem, so the resulting
/// path still spells the location the way git recorded it (symlinks intact).
fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

#[cfg(test)]
#[path = "git_worktree/tests.rs"]
mod tests;
