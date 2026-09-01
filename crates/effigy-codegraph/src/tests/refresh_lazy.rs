use super::*;

use std::time::Duration;

use crate::refresh::{
    ensure_fresh_with_wait_and_progress, run_index_exclusive_with_wait, RefreshLock,
};

#[test]
fn query_refreshes_stale_index_on_demand() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    run_index(temp.path()).expect("index");

    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\npub fn brand_new_symbol() { helper(); }\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    let search_payload = query_search(temp.path(), "brand_new_symbol", Some(10)).expect("search");
    assert!(!search_payload.freshness.stale);
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload.freshness.stale_paths.is_empty());
    assert!(search_payload
        .freshness
        .summary
        .contains("graph auto-refreshed"));
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.record_type == "symbol"
            && entry.name.as_deref() == Some("brand_new_symbol")));

    let store = GraphStore::open(temp.path()).expect("open store");
    assert_eq!(store.list_index_runs().expect("runs").len(), 2);
}

#[test]
fn query_builds_missing_index_on_demand() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    let search_payload = query_search(temp.path(), "run_release", Some(10)).expect("search");
    assert!(search_payload.freshness.usable);
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload.freshness.stale_paths.is_empty());
    assert!(search_payload
        .freshness
        .summary
        .contains("graph index built on demand"));
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("run_release")));
}

#[test]
fn refresh_lock_is_exclusive() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");

    let first = RefreshLock::try_acquire(temp.path()).expect("first acquire");
    assert!(first.is_some());
    assert!(RefreshLock::try_acquire(temp.path())
        .expect("second acquire")
        .is_none());
    drop(first);
    assert!(RefreshLock::try_acquire(temp.path())
        .expect("reacquire")
        .is_some());
}

#[test]
fn explicit_index_refuses_to_run_without_the_refresh_lock() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");

    let _held = RefreshLock::try_acquire(temp.path())
        .expect("hold refresh lock")
        .expect("lock must be free");
    let error = run_index_exclusive_with_wait(temp.path(), 0)
        .expect_err("explicit index must not bypass a held refresh lock");

    assert!(error
        .to_string()
        .contains("graph refresh lock remained busy"));
    let store = GraphStore::open(temp.path()).expect("open store");
    assert_eq!(store.counts().expect("counts").files, 0);
}

#[test]
fn query_serves_stale_when_refresh_lock_is_held() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn changed_symbol() {}\nfn helper() {}\n",
    )
    .expect("rewrite rust");

    let _held = RefreshLock::try_acquire(temp.path())
        .expect("hold refresh lock")
        .expect("lock must be free");

    let store = GraphStore::open(temp.path()).expect("open store");
    let outcome = ensure_fresh_with_wait_and_progress(temp.path(), &store, 250, |_| {})
        .expect("ensure fresh");
    assert!(outcome.freshness.stale);
    assert_eq!(outcome.freshness.state, "refresh-recommended");
    assert!(outcome
        .notes
        .iter()
        .any(|note| note.contains("in progress by another process")));
}

#[test]
fn query_detects_refresh_completed_by_concurrent_process() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn concurrent_symbol() {}\n",
    )
    .expect("rewrite rust");

    let lock_root = temp.path().to_path_buf();
    let handle = std::thread::spawn(move || {
        let lock = RefreshLock::try_acquire(&lock_root)
            .expect("acquire")
            .expect("lock must be free");
        std::thread::sleep(Duration::from_millis(150));
        crate::index::run_index_unlocked(&lock_root).expect("concurrent refresh");
        drop(lock);
    });

    let store = GraphStore::open(temp.path()).expect("open store");
    let outcome = ensure_fresh_with_wait_and_progress(temp.path(), &store, 1_000, |_| {})
        .expect("ensure fresh");
    handle.join().expect("join refresh thread");

    assert!(!outcome.freshness.stale);
    assert_eq!(outcome.freshness.state, "ready");
    assert!(outcome
        .notes
        .iter()
        .any(|note| note.contains("concurrent process")));
    assert!(!outcome
        .notes
        .iter()
        .any(|note| note.contains("auto-refreshed")));
}

#[test]
fn status_stays_report_only_when_queries_auto_refresh() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");

    query_search(temp.path(), "release", Some(10)).expect("search is fresh, no refresh");

    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn later_symbol() {}\n",
    )
    .expect("rewrite rust");

    let status_payload = status(temp.path()).expect("status");
    assert_eq!(status_payload.freshness.state, "refresh-recommended");
    assert!(status_payload
        .stale_paths
        .contains(&"src/lib.rs".to_owned()));

    let search_payload = query_search(temp.path(), "later_symbol", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("later_symbol")));
}
