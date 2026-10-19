use super::*;

use std::time::{Duration, Instant};

use crate::docs_context::DocsContextRequest;
use crate::refresh::{
    ensure_fresh, open_query_store, run_index_exclusive_with_wait, RefreshLock, RefreshPolicy,
    RefreshSource,
};
use crate::scope::GraphScope;

fn repo_scope(root: &Path) -> GraphScope {
    GraphScope::repo_root(root).expect("repo root scope")
}

#[test]
fn query_reports_stale_until_explicit_index() {
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

    let stale = query_search(temp.path(), "brand_new_symbol", Some(10)).expect("stale search");
    assert!(stale.freshness.stale);
    assert!(stale.matches.is_empty());
    run_index(temp.path()).expect("explicit refresh");
    let search_payload = query_search(temp.path(), "brand_new_symbol", Some(10)).expect("search");
    assert!(!search_payload.freshness.stale);
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload.freshness.stale_paths.is_empty());

    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.record_type == "symbol"
            && entry.name.as_deref() == Some("brand_new_symbol")));

    let store = GraphStore::open(temp.path()).expect("open store");
    assert_eq!(store.list_index_runs().expect("runs").len(), 2);
}

#[test]
fn query_leaves_missing_index_uncreated() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    let search_payload = query_search(temp.path(), "run_release", Some(10)).expect("search");
    assert!(!search_payload.freshness.usable);
    assert!(search_payload
        .freshness
        .summary
        .contains("effigy graph index"));
    assert!(!temp.path().join(".effigy").exists());
}

#[test]
fn refresh_lock_is_exclusive() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");

    let first = RefreshLock::try_acquire(&repo_scope(temp.path())).expect("first acquire");
    assert!(first.is_some());
    assert!(RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("second acquire")
        .is_none());
    drop(first);
    assert!(RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("reacquire")
        .is_some());
}

#[test]
fn explicit_index_refuses_to_run_without_the_refresh_lock() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");

    let _held = RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("hold refresh lock")
        .expect("lock must be free");
    let error = run_index_exclusive_with_wait(&repo_scope(temp.path()), 0)
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

    let _held = RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("hold refresh lock")
        .expect("lock must be free");

    let store = GraphStore::open(temp.path()).expect("open store");
    let started = Instant::now();
    let outcome = ensure_fresh(&repo_scope(temp.path()), &store).expect("ensure fresh");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "live holder with a complete snapshot must not spend the in-flight wait"
    );
    assert_eq!(outcome.freshness.state, "refreshing");
    assert!(outcome.freshness.usable);
    assert_eq!(outcome.source, RefreshSource::Live);
    let lock = outcome.freshness.lock.expect("lock identity");
    assert!(lock.held);
    assert_eq!(lock.pid, Some(std::process::id()));
    assert!(!lock.stale_holder);
    assert!(outcome
        .freshness
        .summary
        .contains("serving committed files"));
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

    let (held_sender, held_receiver) = std::sync::mpsc::channel();
    let lock_scope = repo_scope(temp.path());
    let handle = std::thread::spawn(move || {
        let lock = RefreshLock::try_acquire(&lock_scope)
            .expect("acquire")
            .expect("lock must be free");
        held_sender.send(()).expect("signal lock held");
        std::thread::sleep(Duration::from_millis(150));
        crate::index::run_index_unlocked_in_scope(&lock_scope).expect("concurrent refresh");
        drop(lock);
    });

    held_receiver.recv().expect("wait for concurrent holder");
    let store = GraphStore::open(temp.path()).expect("open store");
    let outcome = ensure_fresh(&repo_scope(temp.path()), &store).expect("ensure fresh");
    handle.join().expect("join refresh thread");

    assert!(outcome.freshness.stale);
    assert_eq!(outcome.freshness.state, "refreshing");
    assert!(outcome.freshness.lock.is_some_and(|lock| lock.held));
    let after =
        query_search(temp.path(), "concurrent_symbol", Some(10)).expect("read finished refresh");
    assert_eq!(after.freshness.state, "ready");
    assert!(after
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("concurrent_symbol")));
}

#[test]
fn status_and_queries_stay_report_only() {
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

    let stale = query_search(temp.path(), "later_symbol", Some(10)).expect("stale query");
    assert!(stale.freshness.stale);
    assert!(stale.matches.is_empty());
    run_index(temp.path()).expect("explicit refresh");
    let search_payload = query_search(temp.path(), "later_symbol", Some(10)).expect("search");
    assert_eq!(search_payload.freshness.state, "ready");
    assert!(search_payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("later_symbol")));
}

#[test]
fn refresh_and_query_record_a_readable_graph_phase() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("docs")).expect("mkdir docs");
    for index in 0..4 {
        fs::write(
            temp.path().join(format!("docs/topic-{index}.md")),
            format!("# Topic {index}\n\nThe release notes cover topic {index}.\n"),
        )
        .expect("write doc");
    }

    run_index(temp.path()).expect("index");
    crate::docs_context(temp.path(), "release", DocsContextRequest::default())
        .expect("docs context");

    // The recorder is process-global on purpose: a bounded caller reads it
    // from the reporting thread while the graph worker is still running. So
    // this asserts the shape a timeout reports, not an exact interleaving.
    let snapshot = crate::phase::snapshot().expect("graph work records a phase");
    assert!(
        crate::phase::KNOWN_PHASE_NAMES.contains(&snapshot.name.as_str()),
        "unknown phase name: {}",
        snapshot.name
    );
    match (snapshot.items_done, snapshot.items_total) {
        (Some(done), Some(total)) => assert!(
            done <= total,
            "progress must never exceed its own total: {done}/{total}"
        ),
        (None, None) => {}
        other => panic!("progress must report both bounds or neither: {other:?}"),
    }
}

#[test]
fn live_holder_is_named_and_committed_files_are_served_read_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");
    assert!(!temp.path().join(".effigy/graph/graph.complete.db").exists());
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn live_holder_only() {}\n",
    )
    .expect("rewrite rust");

    let _held = RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("hold refresh lock")
        .expect("lock must be free");

    let started = Instant::now();
    let (store, freshness) = open_query_store(&repo_scope(temp.path()), RefreshPolicy::query())
        .expect("lookup under live holder");
    let store = store.expect("committed files");
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(freshness.state, "refreshing");
    assert!(freshness.usable);
    let lock = freshness.lock.expect("lock identity");
    assert_eq!(lock.pid, Some(std::process::id()));
    assert!(!lock.stale_holder);
    let names = store
        .list_symbols()
        .expect("symbols")
        .into_iter()
        .map(|symbol| symbol.display_name)
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name == "run_release"));
    assert!(
        !names.iter().any(|name| name == "live_holder_only"),
        "must not read the in-progress live database"
    );
}

#[test]
fn stale_index_flag_serves_last_complete_database_without_refresh() {
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
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn after_index() {}\n",
    )
    .expect("rewrite rust");

    let (store, freshness) =
        open_query_store(&repo_scope(temp.path()), RefreshPolicy::lookup(true))
            .expect("stale-index lookup");
    let store = store.expect("committed files");
    assert_eq!(freshness.state, "stale-index");
    assert!(freshness.usable);
    assert!(freshness.summary.contains("not current"));
    let names = store
        .list_symbols()
        .expect("symbols")
        .into_iter()
        .map(|symbol| symbol.display_name)
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name == "run_release"));
    assert!(
        !names.iter().any(|name| name == "after_index"),
        "stale-index must not refresh or read the dirty tree"
    );
}

#[test]
fn cold_worktree_stale_index_returns_immediately_with_next_action() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn cold_only() {}\n").expect("write rust");

    let started = Instant::now();
    let (_store, freshness) =
        open_query_store(&repo_scope(temp.path()), RefreshPolicy::lookup(true))
            .expect("cold stale-index lookup");
    assert!(
        started.elapsed() < Duration::from_millis(1_000),
        "cold --stale-index must not start a first index"
    );
    assert_eq!(freshness.state, "missing-index");
    assert!(!freshness.usable);
    assert!(freshness.summary.contains("effigy graph index"));
}

#[test]
fn lock_held_without_snapshot_does_not_query_live_store() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");

    let _held = RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("hold refresh lock")
        .expect("lock must be free");

    for policy in [RefreshPolicy::query(), RefreshPolicy::lookup(true)] {
        let (store, freshness) = open_query_store(&repo_scope(temp.path()), policy)
            .expect("status-only lookup while cold lock is held");
        assert!(
            store.is_none(),
            "lock-held lookup without a snapshot must not hand out the live store"
        );
        assert_eq!(freshness.state, "refreshing");
        assert!(!freshness.usable);
        assert!(freshness.lock.as_ref().is_some_and(|lock| lock.held));

        let payload =
            crate::search_in_scope(&repo_scope(temp.path()), "run_release", Some(10), policy)
                .expect("search");
        assert!(
            payload.matches.is_empty(),
            "must not search a partial live index: {:?}",
            payload.matches
        );
        assert!(!payload.freshness.usable);
    }
}

#[test]
fn committed_search_covers_initial_index_and_changed_symbol() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");

    let initial = crate::search_in_scope(
        &repo_scope(temp.path()),
        "run_release",
        Some(10),
        RefreshPolicy::lookup(true),
    )
    .expect("search initial snapshot");
    assert_eq!(initial.freshness.state, "stale-index");
    assert!(
        initial
            .matches
            .iter()
            .any(|entry| entry.name.as_deref() == Some("run_release")),
        "initial snapshot FTS missed run_release: {:?}",
        initial.matches
    );

    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn brand_new_symbol() {}\n",
    )
    .expect("rewrite rust");
    run_index(temp.path()).expect("reindex");

    let changed = crate::search_in_scope(
        &repo_scope(temp.path()),
        "brand_new_symbol",
        Some(10),
        RefreshPolicy::lookup(true),
    )
    .expect("search changed snapshot");
    assert_eq!(changed.freshness.state, "stale-index");
    assert!(
        changed
            .matches
            .iter()
            .any(|entry| entry.name.as_deref() == Some("brand_new_symbol")),
        "changed snapshot FTS missed brand_new_symbol: {:?}",
        changed.matches
    );
}

#[test]
fn refresh_never_publishes_a_snapshot() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn first() {}\n").expect("write rust");
    run_index(temp.path()).expect("index");
    let dest = temp.path().join(".effigy/graph/graph.complete.db");
    assert!(!dest.exists(), "refresh must not copy the database");
    fs::write(&dest, "obsolete snapshot").expect("legacy snapshot");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn first() {}\npub fn second() {}\n",
    )
    .expect("rewrite rust");
    run_index(temp.path()).expect("reindex");
    assert!(!dest.exists(), "refresh removes the obsolete snapshot");
    assert!(!fs::read_dir(dest.parent().expect("graph dir"))
        .expect("read graph dir")
        .any(|entry| entry
            .expect("entry")
            .file_name()
            .to_string_lossy()
            .contains("publishing-")));

    let payload = crate::search_in_scope(
        &repo_scope(temp.path()),
        "second",
        Some(10),
        RefreshPolicy::lookup(true),
    )
    .expect("search replaced snapshot");
    assert!(payload
        .matches
        .iter()
        .any(|entry| entry.name.as_deref() == Some("second")));
}
