use std::fs;

use tempfile::TempDir;

use super::{detect_linked_worktree, lexically_normalize, primary_checkout_fallback};

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
