use std::path::Path;

use crate::repo_markers::{LOCAL_OVERLAY_FILE, LOCAL_OVERLAY_GITIGNORE_ALIASES};

pub fn ensure_effigy_ignored_in_git_root(repo_root: &Path) -> std::io::Result<bool> {
    ensure_pattern_ignored_in_git_root(repo_root, ".effigy", &[".effigy", ".effigy/"])
}

/// Append `effigy.local.toml` to the repo's `.gitignore` (creating the
/// file if needed) the first time the auto-discovered local overlay is
/// observed. Idempotent. No-op on non-git roots.
pub fn ensure_local_overlay_ignored_in_git_root(repo_root: &Path) -> std::io::Result<bool> {
    ensure_pattern_ignored_in_git_root(
        repo_root,
        LOCAL_OVERLAY_FILE,
        &LOCAL_OVERLAY_GITIGNORE_ALIASES,
    )
}

fn ensure_pattern_ignored_in_git_root(
    repo_root: &Path,
    append_line: &str,
    accepted_aliases: &[&str],
) -> std::io::Result<bool> {
    if !repo_root.join(".git").is_dir() {
        return Ok(false);
    }

    let gitignore_path = repo_root.join(".gitignore");
    let existing = match std::fs::read_to_string(&gitignore_path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };

    if existing
        .lines()
        .map(str::trim)
        .any(|line| accepted_aliases.contains(&line))
    {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(append_line);
    updated.push('\n');
    std::fs::write(gitignore_path, updated)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{ensure_effigy_ignored_in_git_root, ensure_local_overlay_ignored_in_git_root};

    #[test]
    fn creates_gitignore_when_git_root_has_none() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(root.path().join(".git")).expect("mkdir git");

        let changed = ensure_effigy_ignored_in_git_root(root.path()).expect("ignore");

        assert!(changed);
        assert_eq!(
            std::fs::read_to_string(root.path().join(".gitignore")).expect("read"),
            ".effigy\n"
        );
    }

    #[test]
    fn appends_effigy_without_duplicate() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(root.path().join(".git")).expect("mkdir git");
        std::fs::write(root.path().join(".gitignore"), "target").expect("write gitignore");

        let changed = ensure_effigy_ignored_in_git_root(root.path()).expect("ignore");
        let second = ensure_effigy_ignored_in_git_root(root.path()).expect("ignore again");

        assert!(changed);
        assert!(!second);
        assert_eq!(
            std::fs::read_to_string(root.path().join(".gitignore")).expect("read"),
            "target\n.effigy\n"
        );
    }

    #[test]
    fn skips_non_git_roots() {
        let root = tempfile::tempdir().expect("tempdir");

        let changed = ensure_effigy_ignored_in_git_root(root.path()).expect("ignore");

        assert!(!changed);
        assert!(!root.path().join(".gitignore").exists());
    }

    #[test]
    fn local_overlay_is_appended_once() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(root.path().join(".git")).expect("mkdir git");

        let changed = ensure_local_overlay_ignored_in_git_root(root.path()).expect("ignore");
        let again = ensure_local_overlay_ignored_in_git_root(root.path()).expect("ignore again");

        assert!(changed);
        assert!(!again);
        assert_eq!(
            std::fs::read_to_string(root.path().join(".gitignore")).expect("read"),
            "effigy.local.toml\n"
        );
    }

    #[test]
    fn local_overlay_skips_when_alias_present() {
        let root = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir(root.path().join(".git")).expect("mkdir git");
        std::fs::write(root.path().join(".gitignore"), "/effigy.local.toml\n")
            .expect("seed gitignore");

        let changed = ensure_local_overlay_ignored_in_git_root(root.path()).expect("ignore");

        assert!(!changed);
    }
}
