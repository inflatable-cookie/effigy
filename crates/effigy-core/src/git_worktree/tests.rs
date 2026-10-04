use std::fs;

use tempfile::TempDir;

use super::{
    core_bare_is_true, detect_linked_worktree, lexically_normalize, primary_checkout_fallback,
    resolve_common_git_dir,
};

struct WorktreeFixture {
    _root: TempDir,
    primary: std::path::PathBuf,
    worktree: std::path::PathBuf,
}

fn linked_worktree_fixture() -> WorktreeFixture {
    let root = TempDir::new().expect("tempdir");
    let primary = root.path().join("primary");
    let worktree = root.path().join("worktrees/feature");
    let worktree_git_dir = primary.join(".git/worktrees/feature");
    fs::create_dir_all(&worktree_git_dir).expect("worktree git dir");
    fs::create_dir_all(&worktree).expect("worktree root");
    fs::write(worktree_git_dir.join("commondir"), "../..\n").expect("commondir");
    fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", worktree_git_dir.display()),
    )
    .expect("gitdir pointer");
    WorktreeFixture {
        _root: root,
        primary,
        worktree,
    }
}

#[test]
fn detect_linked_worktree_resolves_common_dir_and_primary_root() {
    let fixture = linked_worktree_fixture();
    let layout = detect_linked_worktree(&fixture.worktree).expect("linked worktree");
    assert_eq!(
        layout.worktree_git_dir,
        fixture.primary.join(".git/worktrees/feature")
    );
    assert_eq!(layout.common_git_dir, fixture.primary.join(".git"));
    assert_eq!(
        layout.primary_checkout_root.as_deref(),
        Some(fixture.primary.as_path())
    );
}

#[test]
fn detect_linked_worktree_ignores_a_normal_checkout() {
    let root = TempDir::new().expect("tempdir");
    fs::create_dir_all(root.path().join(".git")).expect("git dir");
    assert!(detect_linked_worktree(root.path()).is_none());
}

#[test]
fn detect_linked_worktree_ignores_a_dangling_pointer() {
    let root = TempDir::new().expect("tempdir");
    fs::write(root.path().join(".git"), "gitdir: /nope/does/not/exist\n").expect("pointer");
    assert!(detect_linked_worktree(root.path()).is_none());
}

#[test]
fn primary_checkout_fallback_returns_an_existing_primary_copy() {
    let fixture = linked_worktree_fixture();
    let vault = fixture.primary.join(".effigy/secrets/local.vault");
    fs::create_dir_all(vault.parent().expect("vault parent")).expect("vault dir");
    fs::write(&vault, "{}").expect("vault");
    assert_eq!(
        primary_checkout_fallback(
            &fixture.worktree,
            std::path::Path::new(".effigy/secrets/local.vault")
        ),
        Some(vault)
    );
}

#[test]
fn primary_checkout_fallback_is_none_when_the_primary_copy_is_absent() {
    let fixture = linked_worktree_fixture();
    assert!(primary_checkout_fallback(
        &fixture.worktree,
        std::path::Path::new(".effigy/secrets/local.vault")
    )
    .is_none());
}

#[test]
fn lexically_normalize_collapses_parent_segments() {
    assert_eq!(
        lexically_normalize(std::path::Path::new("/a/b/.git/worktrees/x/../..")),
        std::path::PathBuf::from("/a/b/.git")
    );
}

#[test]
fn resolve_common_git_dir_uses_a_real_git_directory() {
    let root = TempDir::new().expect("tempdir");
    fs::create_dir_all(root.path().join(".git")).expect("git dir");
    assert_eq!(
        resolve_common_git_dir(root.path()).expect("resolve"),
        Some(root.path().join(".git"))
    );
}

#[test]
fn resolve_common_git_dir_is_none_without_a_git_marker() {
    let root = TempDir::new().expect("tempdir");
    assert_eq!(resolve_common_git_dir(root.path()).expect("resolve"), None);
}

#[test]
fn resolve_common_git_dir_follows_a_linked_worktree_commondir() {
    let fixture = linked_worktree_fixture();
    fs::write(fixture.primary.join(".git/HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    assert_eq!(
        resolve_common_git_dir(&fixture.worktree).expect("resolve"),
        Some(fixture.primary.join(".git"))
    );
}

#[test]
fn resolve_common_git_dir_rejects_a_dangling_gitfile() {
    let root = TempDir::new().expect("tempdir");
    fs::write(root.path().join(".git"), "gitdir: /nope/does/not/exist\n").expect("pointer");
    let error = resolve_common_git_dir(root.path()).expect_err("dangling");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn resolve_common_git_dir_rejects_a_gitfile_whose_common_dir_has_no_head() {
    let fixture = linked_worktree_fixture();
    let error = resolve_common_git_dir(&fixture.worktree).expect_err("no HEAD");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn resolve_common_git_dir_rejects_a_gitfile_pointing_at_a_bare_directory() {
    let root = TempDir::new().expect("tempdir");
    let checkout = root.path().join("checkout");
    let bare = root.path().join("bare");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&bare).expect("bare");
    fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(
        bare.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = true\n",
    )
    .expect("config");
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", bare.display()),
    )
    .expect("gitfile");

    let error = resolve_common_git_dir(&checkout).expect_err("bare gitfile");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("bare"));
}

#[test]
fn resolve_common_git_dir_rejects_last_wins_core_bare() {
    let root = TempDir::new().expect("tempdir");
    let checkout = root.path().join("checkout");
    let bare = root.path().join("bare");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&bare).expect("bare");
    fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(
        bare.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tbare = true\n",
    )
    .expect("config");
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", bare.display()),
    )
    .expect("gitfile");

    let error = resolve_common_git_dir(&checkout).expect_err("last-wins bare");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[cfg(unix)]
#[test]
fn resolve_common_git_dir_rejects_a_symlink_to_a_bare_directory() {
    let root = TempDir::new().expect("tempdir");
    let checkout = root.path().join("checkout");
    let bare = root.path().join("bare");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&bare).expect("bare");
    fs::write(bare.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(
        bare.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = true\n",
    )
    .expect("config");
    std::os::unix::fs::symlink(&bare, checkout.join(".git")).expect("symlink .git");

    let error = resolve_common_git_dir(&checkout).expect_err("symlink bare");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("bare"));
}

#[cfg(unix)]
#[test]
fn resolve_common_git_dir_accepts_a_symlink_to_a_non_bare_git_dir() {
    let root = TempDir::new().expect("tempdir");
    let checkout = root.path().join("checkout");
    let admin = root.path().join("admin");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&admin).expect("admin");
    fs::write(admin.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(
        admin.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n",
    )
    .expect("config");
    std::os::unix::fs::symlink(&admin, checkout.join(".git")).expect("symlink .git");

    assert_eq!(
        resolve_common_git_dir(&checkout).expect("resolve"),
        Some(admin.canonicalize().expect("canonical admin"))
    );
}

#[test]
fn resolve_common_git_dir_accepts_a_separate_git_dir_pointer() {
    let root = TempDir::new().expect("tempdir");
    let checkout = root.path().join("checkout");
    let admin = root.path().join("admin");
    fs::create_dir_all(&checkout).expect("checkout");
    fs::create_dir_all(&admin).expect("admin");
    fs::write(admin.join("HEAD"), "ref: refs/heads/main\n").expect("HEAD");
    fs::write(
        admin.join("config"),
        "[core]\n\trepositoryformatversion = 0\n\tbare = false\n\tworktree = ../checkout\n",
    )
    .expect("config");
    fs::write(
        checkout.join(".git"),
        format!("gitdir: {}\n", admin.display()),
    )
    .expect("gitfile");

    assert_eq!(
        resolve_common_git_dir(&checkout).expect("resolve"),
        Some(admin)
    );
}

#[test]
fn core_bare_true_matches_git_booleans_and_ignores_other_sections() {
    assert!(core_bare_is_true("[core]\n\tbare = true\n"));
    assert!(core_bare_is_true("[core]\nbare = YES\n"));
    assert!(core_bare_is_true("[core]\nbare = on # comment\n"));
    assert!(core_bare_is_true("[core]\nbare = 1\n"));
    assert!(core_bare_is_true("[core]\nbare\n"));
    assert!(!core_bare_is_true("[core]\n\tbare = false\n"));
    assert!(!core_bare_is_true(
        "[core]\n\trepositoryformatversion = 0\n"
    ));
    assert!(!core_bare_is_true("[remote \"origin\"]\nbare = true\n"));
    assert!(!core_bare_is_true(""));
    assert!(core_bare_is_true("[core]\n\tbare = false\n\tbare = true\n"));
    assert!(!core_bare_is_true(
        "[core]\n\tbare = true\n\tbare = false\n"
    ));
    assert!(core_bare_is_true(
        "[core]\n\tbare = false\n[other]\n\tbare = false\n[core]\n\tbare = true\n"
    ));
}
