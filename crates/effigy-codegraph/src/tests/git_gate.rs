use super::*;

use std::path::Path;
use std::process::Command;

use crate::git::{git_gate_says_fresh, GIT_INDEXED_HEAD_KEY};

fn git(repo_root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git should run");
    assert!(status.success(), "git {args:?} failed");
}

fn git_init_and_commit(repo_root: &Path) {
    git(repo_root, &["init", "-q", "-b", "main"]);
    git(repo_root, &["add", "-A"]);
    git(
        repo_root,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
}

fn write_rust_with_extra_symbol(repo_root: &Path, extra: &str) {
    fs::write(
        repo_root.join("src/lib.rs"),
        format!("pub fn run_release() {{ helper(); }}\nfn helper() {{}}\n{extra}\n"),
    )
    .expect("rewrite rust");
}

#[test]
fn run_index_stamps_clean_git_head_and_gate_fires() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_rust_with_extra_symbol(root, "");
    git_init_and_commit(root);

    run_index(root).expect("index");

    let store = GraphStore::open(root).expect("open store");
    let stamped_head = store
        .metadata_value(GIT_INDEXED_HEAD_KEY)
        .expect("stamp")
        .expect("stamp must be set");
    let head = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .current_dir(root)
        .output()
        .expect("git head");
    assert_eq!(
        stamped_head,
        String::from_utf8(head.stdout).expect("utf8").trim()
    );
    assert!(git_gate_says_fresh(root, &store).expect("gate"));

    let search_payload = query_search(root, "run_release", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(store.list_index_runs().expect("runs").len() == 1);
}

#[test]
fn gate_turns_off_when_head_moves() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_rust_with_extra_symbol(root, "");
    git_init_and_commit(root);
    run_index(root).expect("index");

    write_rust_with_extra_symbol(root, "pub fn committed_later() {}");
    git(root, &["add", "-A"]);
    git(
        root,
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=test",
            "commit",
            "-q",
            "-m",
            "next",
        ],
    );

    let store = GraphStore::open(root).expect("open store");
    assert!(!git_gate_says_fresh(root, &store).expect("gate"));

    let search_payload = query_search(root, "committed_later", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("committed_later")));
}

#[test]
fn run_index_clears_stamp_on_dirty_tree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_rust_with_extra_symbol(root, "");
    git_init_and_commit(root);
    run_index(root).expect("index");

    let store = GraphStore::open(root).expect("open store");
    assert!(git_gate_says_fresh(root, &store).expect("gate initially"));

    write_rust_with_extra_symbol(root, "pub fn dirty_symbol() {}");
    run_index(root).expect("index over dirty tree");

    let store = GraphStore::open(root).expect("reopen store");
    assert!(!git_gate_says_fresh(root, &store).expect("gate off on dirty index"));

    // Revert the edit: the tree is clean again at the original HEAD, but the
    // indexed content was dirty — the walk must still catch it.
    git(root, &["checkout", "-q", "--", "src/lib.rs"]);
    let search_payload = query_search(root, "dirty_symbol", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(!search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("dirty_symbol")));
}

#[test]
fn gate_detects_dirty_tree_after_stamp() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_rust_with_extra_symbol(root, "");
    git_init_and_commit(root);
    run_index(root).expect("index");

    write_rust_with_extra_symbol(root, "pub fn uncommitted_symbol() {}");

    let store = GraphStore::open(root).expect("open store");
    assert!(!git_gate_says_fresh(root, &store).expect("gate off on dirty tree"));

    let search_payload = query_search(root, "uncommitted_symbol", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("uncommitted_symbol")));
}

#[test]
fn non_git_repo_has_no_stamp_and_queries_still_refresh() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::create_dir_all(root.join("src")).expect("mkdir src");
    write_rust_with_extra_symbol(root, "");
    run_index(root).expect("index");

    let store = GraphStore::open(root).expect("open store");
    assert!(store
        .metadata_value(GIT_INDEXED_HEAD_KEY)
        .expect("stamp")
        .is_none());
    assert!(!git_gate_says_fresh(root, &store).expect("gate off without git"));

    write_rust_with_extra_symbol(root, "pub fn late_symbol() {}");
    let search_payload = query_search(root, "late_symbol", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("late_symbol")));
}
