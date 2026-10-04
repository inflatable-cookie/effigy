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
//! `$GIT_COMMON_DIR` is the shared admin directory (`info/exclude` lives
//! there). A linked worktree's private git directory is not a substitute:
//! Git reads `info/` from the common dir. A `.git` file without `commondir`
//! is a separate-git-dir checkout; the pointer itself is the admin dir.
//!
//! Resolution is pure filesystem reading — no `git` subprocess — so it works
//! inside minimal containers and costs two small reads.

use std::ffi::OsStr;
use std::fs;
use std::io;
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

/// Resolve `$GIT_COMMON_DIR` for a working tree at `repo_root`.
///
/// - No `.git` marker: `Ok(None)` (not a Git working tree, including a bare
///   repo passed as the root).
/// - `.git` is a real directory: that path, constructed from `repo_root`.
/// - `.git` is a file (`gitdir:`): the shared common dir when `commondir`
///   exists, otherwise the pointer target (separate-git-dir). The target must
///   exist as a directory that contains `HEAD` and is not `core.bare`. An
///   unreadable, dangling, or bare marker is `InvalidData`, not a missing repo.
///
/// Never creates `.git`. Paths come from the marker and Git's `commondir`
/// file; callers must not invent sibling, home, or working-tree fallbacks.
pub fn resolve_common_git_dir(repo_root: &Path) -> io::Result<Option<PathBuf>> {
    let marker = repo_root.join(".git");
    let metadata = match fs::symlink_metadata(&marker) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };

    if metadata.is_dir() {
        return Ok(Some(marker));
    }

    if metadata.file_type().is_symlink() && marker.is_dir() {
        let canonical = fs::canonicalize(&marker)?;
        return require_git_admin_dir(&canonical).map(Some);
    }

    if metadata.is_file() || (metadata.file_type().is_symlink() && marker.is_file()) {
        return resolve_gitfile_common_dir(repo_root);
    }

    Err(invalid_git_marker(repo_root, "unexpected .git marker type"))
}

fn resolve_gitfile_common_dir(repo_root: &Path) -> io::Result<Option<PathBuf>> {
    let Some(layout) = detect_linked_worktree(repo_root) else {
        return Err(invalid_git_marker(
            repo_root,
            "linked worktree metadata is invalid",
        ));
    };
    let git_dir = require_git_admin_dir(&layout.common_git_dir)?;
    if git_dir_is_bare(&git_dir)? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "gitfile at {} points at a bare git directory {}",
                repo_root.display(),
                git_dir.display()
            ),
        ));
    }
    Ok(Some(git_dir))
}

fn require_git_admin_dir(git_dir: &Path) -> io::Result<PathBuf> {
    if !git_dir.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("git admin directory is missing at {}", git_dir.display()),
        ));
    }
    if !git_dir.join("HEAD").exists() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "git admin directory at {} is not a git directory",
                git_dir.display()
            ),
        ));
    }
    Ok(git_dir.to_path_buf())
}

/// Local `core.bare` only. Missing config is not bare. Unreadable config fails.
/// Duplicate keys last-win, matching Git.
fn git_dir_is_bare(git_dir: &Path) -> io::Result<bool> {
    let raw = match fs::read_to_string(git_dir.join("config")) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(core_bare_is_true(&raw))
}

fn core_bare_is_true(raw: &str) -> bool {
    let mut in_core = false;
    let mut bare = false;
    for line in raw.lines() {
        let line = strip_unquoted_config_comment(line).trim();
        if line.is_empty() {
            continue;
        }
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            in_core = header.trim().eq_ignore_ascii_case("core");
            continue;
        }
        if !in_core {
            continue;
        }
        match line.split_once('=') {
            Some((name, value)) if name.trim().eq_ignore_ascii_case("bare") => {
                bare = git_bool_is_true(value.trim());
            }
            None if line.eq_ignore_ascii_case("bare") => bare = true,
            _ => {}
        }
    }
    bare
}

fn git_bool_is_true(value: &str) -> bool {
    let value = value.trim_matches('"');
    value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
        || value == "1"
}

fn strip_unquoted_config_comment(line: &str) -> &str {
    let mut in_quotes = false;
    for (index, ch) in line.char_indices() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '#' | ';' if !in_quotes => return &line[..index],
            _ => {}
        }
    }
    line
}

fn invalid_git_marker(repo_root: &Path, reason: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{reason} at {}", repo_root.display()),
    )
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
