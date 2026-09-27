use super::*;

use std::time::{Duration, Instant};

use crate::docs_context::DocsContextRequest;
use crate::refresh::{
    ensure_fresh_with_wait_and_progress, inspect_refresh_lock, open_query_store,
    run_index_exclusive_with_wait, RefreshLock, RefreshPolicy, RefreshSource,
};
use crate::scope::GraphScope;

fn repo_scope(root: &Path) -> GraphScope {
    GraphScope::repo_root(root).expect("repo root scope")
}

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
    let outcome = ensure_fresh_with_wait_and_progress(
        &repo_scope(temp.path()),
        &store,
        2_500,
        RefreshPolicy::query(),
        |_| {},
    )
    .expect("ensure fresh");
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "live holder with a complete snapshot must not spend the in-flight wait"
    );
    assert_eq!(outcome.freshness.state, "stale-index");
    assert!(outcome.freshness.usable);
    assert_eq!(outcome.source, RefreshSource::CompleteSnapshot);
    let lock = outcome.freshness.lock.expect("lock identity");
    assert!(lock.held);
    assert_eq!(lock.pid, Some(std::process::id()));
    assert!(!lock.stale_holder);
    assert!(outcome
        .notes
        .iter()
        .any(|note| note.contains("last complete index")));
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
    fs::remove_file(repo_scope(temp.path()).paths().complete_db_path)
        .expect("drop snapshot so the waiter cannot short-circuit");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\npub fn concurrent_symbol() {}\n",
    )
    .expect("rewrite rust");

    let lock_scope = repo_scope(temp.path());
    let handle = std::thread::spawn(move || {
        let lock = RefreshLock::try_acquire(&lock_scope)
            .expect("acquire")
            .expect("lock must be free");
        std::thread::sleep(Duration::from_millis(150));
        crate::index::run_index_unlocked_in_scope(&lock_scope).expect("concurrent refresh");
        drop(lock);
    });

    let store = GraphStore::open(temp.path()).expect("open store");
    let outcome = ensure_fresh_with_wait_and_progress(
        &repo_scope(temp.path()),
        &store,
        1_000,
        RefreshPolicy::query(),
        |_| {},
    )
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
fn live_holder_with_complete_snapshot_is_named_and_served_read_only() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn run_release() { helper(); }\nfn helper() {}\n",
    )
    .expect("write rust");
    run_index(temp.path()).expect("index");
    assert!(repo_scope(temp.path()).paths().complete_db_path.is_file());
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
    let store = store.expect("complete snapshot");
    assert!(started.elapsed() < Duration::from_millis(500));
    assert_eq!(freshness.state, "stale-index");
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
fn stale_holder_does_not_consume_the_in_flight_wait() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn alpha() {}\n").expect("write rust");
    run_index(temp.path()).expect("index");
    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn alpha() {}\npub fn beta() {}\n",
    )
    .expect("rewrite rust");

    let mut held = RefreshLock::try_acquire(&repo_scope(temp.path()))
        .expect("hold refresh lock")
        .expect("lock must be free");
    // A pid that is a valid positive `i32` and almost certainly unused. `u32::MAX`
    // casts to `-1`, which `kill` treats as a process-group broadcast.
    const DEAD_PID: u32 = 2_000_000;
    held.plant_stale_identity(DEAD_PID).expect("plant dead pid");

    let inspection = inspect_refresh_lock(&repo_scope(temp.path()).paths().refresh_lock_path);
    assert!(inspection.held);
    assert!(inspection
        .identity
        .as_ref()
        .is_some_and(|identity| identity.stale_holder && identity.pid == DEAD_PID));

    let store = GraphStore::open(temp.path()).expect("open store");
    let started = Instant::now();
    let outcome = ensure_fresh_with_wait_and_progress(
        &repo_scope(temp.path()),
        &store,
        2_500,
        RefreshPolicy::query(),
        |_| {},
    )
    .expect("ensure fresh");
    assert!(
        started.elapsed() < Duration::from_millis(800),
        "stale holder must not spend the full in-flight wait"
    );
    assert_eq!(outcome.freshness.state, "stale-index");
    assert!(outcome
        .freshness
        .lock
        .as_ref()
        .is_some_and(|lock| lock.stale_holder && lock.pid == Some(DEAD_PID)));
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
    let store = store.expect("complete snapshot");
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
        assert_eq!(freshness.state, "missing-index");
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
fn snapshot_search_covers_initial_index_and_changed_symbol() {
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
fn snapshot_replace_keeps_prior_file_and_leaves_no_temp() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir src");
    fs::write(temp.path().join("src/lib.rs"), "pub fn first() {}\n").expect("write rust");
    run_index(temp.path()).expect("index");
    let dest = repo_scope(temp.path()).paths().complete_db_path;
    assert!(dest.is_file(), "first snapshot missing");
    let first_len = fs::metadata(&dest).expect("stat first snapshot").len();
    assert!(first_len > 0);

    fs::write(
        temp.path().join("src/lib.rs"),
        "pub fn first() {}\npub fn second() {}\n",
    )
    .expect("rewrite rust");
    run_index(temp.path()).expect("reindex");
    assert!(
        dest.is_file(),
        "replacement must publish over the prior snapshot"
    );
    let graph_dir = dest.parent().expect("graph dir");
    let leftovers = fs::read_dir(graph_dir)
        .expect("read graph dir")
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().contains("publishing-"))
        .count();
    assert_eq!(leftovers, 0, "publishing temp files must not remain");

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
