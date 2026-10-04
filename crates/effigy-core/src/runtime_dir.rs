use std::fs::{self, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::git_worktree::resolve_common_git_dir;
use crate::repo_markers::{LOCAL_OVERLAY_FILE, LOCAL_OVERLAY_GITIGNORE_ALIASES};

/// Ensure `.effigy` is ignored through Git's local exclude file
/// (`$GIT_COMMON_DIR/info/exclude`). Does not create or amend `.gitignore`.
///
/// `info/exclude` is shared by every worktree of that repository. This is
/// Git's common exclude, not a worktree-private file. No-op when `repo_root`
/// is not a Git working tree. An invalid `.git` file, a `.git` symlink to a
/// bare admin dir, or an unwritable admin path fails; there is no
/// working-tree fallback.
pub fn ensure_effigy_ignored_in_git_root(repo_root: &Path) -> io::Result<bool> {
    ensure_pattern_ignored_in_git_root(repo_root, ".effigy", &[".effigy", ".effigy/"])
}

/// Append `effigy.local.toml` to Git's local exclude file the first time the
/// auto-discovered local overlay is observed. Idempotent. No-op on non-git
/// roots. Never amends `.gitignore`.
pub fn ensure_local_overlay_ignored_in_git_root(repo_root: &Path) -> io::Result<bool> {
    ensure_pattern_ignored_in_git_root(
        repo_root,
        LOCAL_OVERLAY_FILE,
        &LOCAL_OVERLAY_GITIGNORE_ALIASES,
    )
}

/// Path used in caller diagnostics when ignore registration fails.
///
/// Prefer the resolved common exclude file. When `repo_root` is not a Git
/// working tree, keep the ordinary `.git/info/exclude` spelling. When
/// resolution fails and `.git` is a file or symlink, report that marker;
/// never invent `.git/info/exclude` through a gitfile or symlink.
pub fn git_local_exclude_path(repo_root: &Path) -> PathBuf {
    match resolve_common_git_dir(repo_root) {
        Ok(Some(git_dir)) => git_dir.join("info").join("exclude"),
        Ok(None) => repo_root.join(".git").join("info").join("exclude"),
        Err(_) => diagnostic_path_for_unresolved_git_marker(repo_root),
    }
}

fn diagnostic_path_for_unresolved_git_marker(repo_root: &Path) -> PathBuf {
    let marker = repo_root.join(".git");
    match fs::symlink_metadata(&marker) {
        Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => marker,
        _ => marker.join("info").join("exclude"),
    }
}

fn ensure_pattern_ignored_in_git_root(
    repo_root: &Path,
    append_line: &str,
    accepted_aliases: &[&str],
) -> io::Result<bool> {
    let Some(git_dir) = resolve_common_git_dir(repo_root)? else {
        return Ok(false);
    };
    let exclude_path = exclude_file_under_git_dir(&git_dir)?;
    append_ignore_line(&exclude_path, append_line, accepted_aliases)
}

fn exclude_file_under_git_dir(git_dir: &Path) -> io::Result<PathBuf> {
    let info_dir = git_dir.join("info");
    match fs::create_dir(&info_dir) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    ensure_admin_path_stays_inside(git_dir, &info_dir, true)?;
    let exclude_path = info_dir.join("exclude");
    ensure_admin_path_stays_inside(git_dir, &exclude_path, false)?;
    Ok(exclude_path)
}

fn ensure_admin_path_stays_inside(
    git_dir: &Path,
    path: &Path,
    must_be_dir: bool,
) -> io::Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound && !must_be_dir => return Ok(()),
        Err(error) => return Err(error),
    };
    if metadata.file_type().is_symlink() {
        if !must_be_dir {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("git exclude path is a symlink at {}", path.display()),
            ));
        }
        let canonical_git = fs::canonicalize(git_dir)?;
        let canonical_path = fs::canonicalize(path)?;
        if !canonical_path.starts_with(&canonical_git) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "git admin path {} escapes git directory {}",
                    path.display(),
                    git_dir.display()
                ),
            ));
        }
        if !canonical_path.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("git info path is not a directory at {}", path.display()),
            ));
        }
        return Ok(());
    }
    if must_be_dir && !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("git info path is not a directory at {}", path.display()),
        ));
    }
    if !must_be_dir && !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("git exclude path is not a file at {}", path.display()),
        ));
    }
    Ok(())
}

fn append_ignore_line(
    exclude_path: &Path,
    append_line: &str,
    accepted_aliases: &[&str],
) -> io::Result<bool> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(exclude_path)?;
    file.lock()?;
    let mut existing = String::new();
    file.read_to_string(&mut existing)?;
    if pattern_already_listed(&existing, append_line, accepted_aliases) {
        return Ok(false);
    }
    file.seek(SeekFrom::End(0))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.write_all(append_line.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(true)
}

fn pattern_already_listed(existing: &str, append_line: &str, accepted_aliases: &[&str]) -> bool {
    existing
        .lines()
        .map(str::trim)
        .any(|line| !line.is_empty() && (line == append_line || accepted_aliases.contains(&line)))
}

#[cfg(test)]
#[path = "runtime_dir/tests.rs"]
mod tests;
