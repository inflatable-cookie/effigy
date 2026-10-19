use super::*;
use std::process::Command;
use std::time::Duration;

type Pause = (
    String,
    std::sync::mpsc::SyncSender<()>,
    std::sync::mpsc::Receiver<()>,
);
thread_local! {
    static PAUSE_AFTER: std::cell::RefCell<Option<Pause>> = const { std::cell::RefCell::new(None) };
    static EXPIRE_AFTER: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn pause_after(
    path: &str,
    ready: std::sync::mpsc::SyncSender<()>,
    resume: std::sync::mpsc::Receiver<()>,
) {
    PAUSE_AFTER.with(|cell| *cell.borrow_mut() = Some((path.into(), ready, resume)));
}
pub(crate) fn boundary(path: &str, point: &str) {
    PAUSE_AFTER.with(|cell| {
        if point == "committed" && cell.borrow().as_ref().is_some_and(|pause| pause.0 == path) {
            let (_, ready, resume) = cell.borrow_mut().take().unwrap();
            ready.send(()).unwrap();
            resume
                .recv_timeout(Duration::from_secs(60))
                .expect("bounded reader proof");
        }
    });
    EXPIRE_AFTER.with(|target| {
        if point == "committed" && target.borrow().as_deref() == Some(path) {
            crate::deadline::expire();
        }
    });
    if std::env::var("EFFIGY_TEST_GRAPH_CRASH_FILE").as_deref() == Ok(path)
        && std::env::var("EFFIGY_TEST_GRAPH_CRASH_POINT").as_deref() == Ok(point)
    {
        // No Rust destructors run. SQLite must recover the abandoned write.
        std::process::exit(73);
    }
}

#[test]
fn crash_child() {
    let Some(root) = std::env::var_os("EFFIGY_TEST_GRAPH_CRASH_ROOT") else {
        return;
    };
    run_index(Path::new(&root)).expect("child refresh");
    panic!("crash injection was not reached");
}

fn crash_refresh(root: &Path, file: &str, point: &str) {
    let child = Command::new(std::env::current_exe().expect("test executable"))
        .args([
            "--exact",
            "tests::file_transactions::crash_child",
            "--nocapture",
        ])
        .env("EFFIGY_TEST_GRAPH_CRASH_ROOT", root)
        .env("EFFIGY_TEST_GRAPH_CRASH_FILE", file)
        .env("EFFIGY_TEST_GRAPH_CRASH_POINT", point)
        .output()
        .expect("crash child");
    assert_eq!(
        child.status.code(),
        Some(73),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
}

fn write_version(root: &Path, name: &str, refreshed: bool) {
    let content = if refreshed {
        format!("pub fn refreshed_{name}_long() {{}}\n")
    } else {
        format!("pub fn old_{name}() {{}}\n")
    };
    fs::write(root.join(format!("{name}.rs")), content).expect("write version");
}

fn assert_file_consistent(root: &Path, name: &str, refreshed: bool) {
    let store = GraphStore::open(root).expect("reopen after crash");
    let path = format!("{name}.rs");
    let file = store
        .list_files()
        .expect("files")
        .into_iter()
        .find(|file| file.path == path)
        .expect("file");
    let state = store
        .file_scan_state_map()
        .expect("states")
        .remove(&path)
        .expect("state");
    assert_eq!(file.content_hash, state.content_hash);
    let expected = if refreshed {
        format!("refreshed_{name}_long")
    } else {
        format!("old_{name}")
    };
    let symbols = store.list_symbols().expect("symbols");
    let owned: Vec<_> = symbols
        .iter()
        .filter(|symbol| symbol.file_id == file.id)
        .collect();
    assert_eq!(owned.len(), 1);
    assert_eq!(owned[0].display_name, expected);
    assert_eq!(store.search(&expected, 10).expect("search").len(), 1);
    let source_hits = store.source_search(&expected, 10).expect("source search");
    assert_eq!(source_hits.len(), 1);
    assert_eq!(source_hits[0].file_id, file.id.as_str());
}

#[test]
fn crash_at_every_file_boundary_preserves_rows_search_and_staleness() {
    let names = ["a", "b", "c"];
    for (position, name) in names.iter().enumerate() {
        for point in ["deleted", "graph", "search", "state", "committed"] {
            let temp = tempfile::tempdir().expect("private fixture");
            for name in names {
                write_version(temp.path(), name, false);
            }
            run_index(temp.path()).expect("baseline");
            for name in names {
                write_version(temp.path(), name, true);
            }
            crash_refresh(temp.path(), &format!("{name}.rs"), point);
            let committed = position + usize::from(point == "committed");
            for (index, name) in names.iter().enumerate() {
                assert_file_consistent(temp.path(), name, index < committed);
            }
            let report = status(temp.path()).expect("status after crash");
            let expected_stale: Vec<_> = names[committed..]
                .iter()
                .map(|name| format!("{name}.rs"))
                .collect();
            assert_eq!(
                report.stale_paths, expected_stale,
                "file {name}, point {point}"
            );
            assert!(!temp.path().join(".effigy/graph/graph.complete.db").exists());
            run_index(temp.path()).expect("resume");
            assert!(status(temp.path())
                .expect("resumed status")
                .stale_paths
                .is_empty());
            for name in names {
                assert_file_consistent(temp.path(), name, true);
            }
        }
    }
}

#[test]
fn crash_during_deletion_keeps_graph_search_and_state_together() {
    for point in ["delete-graph", "delete-state", "delete-committed"] {
        let temp = tempfile::tempdir().expect("private fixture");
        write_version(temp.path(), "a", false);
        run_index(temp.path()).expect("baseline");
        fs::remove_file(temp.path().join("a.rs")).expect("delete source");
        crash_refresh(temp.path(), "a.rs", point);
        let store = GraphStore::open(temp.path()).expect("reopen");
        let expected = usize::from(point != "delete-committed");
        assert_eq!(store.counts().expect("counts").files, expected);
        assert_eq!(store.file_scan_state_map().expect("states").len(), expected);
        assert_eq!(store.search("old_a", 10).expect("search").len(), expected);
        assert_eq!(
            store
                .source_search("old_a", 10)
                .expect("source search")
                .len(),
            expected
        );
        assert_eq!(
            status(temp.path()).expect("status").stale_paths.len(),
            expected
        );
        drop(store);
        run_index(temp.path()).expect("resume deletion");
        assert!(status(temp.path()).expect("resumed").stale_paths.is_empty());
    }
}

#[test]
fn deadline_after_each_file_commits_progress_and_resumes() {
    for name in ["a", "b", "c"] {
        let temp = tempfile::tempdir().expect("private fixture");
        for name in ["a", "b", "c"] {
            write_version(temp.path(), name, true);
        }
        EXPIRE_AFTER.with(|target| *target.borrow_mut() = Some(format!("{name}.rs")));
        let result =
            crate::deadline::with_deadline(Duration::from_secs(60), || run_index(temp.path()));
        EXPIRE_AFTER.with(|target| *target.borrow_mut() = None);
        assert!(matches!(result, Err(CodeGraphError::DeadlineExceeded)));
        let expected_stale: Vec<_> = ["a", "b", "c"]
            .into_iter()
            .filter(|candidate| *candidate > name)
            .map(|name| format!("{name}.rs"))
            .collect();
        assert_eq!(
            status(temp.path()).expect("status").stale_paths,
            expected_stale
        );
        let committed = GraphStore::open(temp.path())
            .expect("store")
            .counts()
            .expect("counts")
            .files;
        assert_eq!(committed, 3 - expected_stale.len());
        run_index(temp.path()).expect("resume");
        assert!(status(temp.path()).expect("resumed").stale_paths.is_empty());
        for name in ["a", "b", "c"] {
            assert_file_consistent(temp.path(), name, true);
        }
    }
}

#[test]
fn refreshing_a_target_preserves_an_unchanged_files_incoming_edges() {
    let temp = tempfile::tempdir().expect("private fixture");
    write_version(temp.path(), "a", false);
    write_version(temp.path(), "b", false);
    run_index(temp.path()).expect("baseline");
    let store = GraphStore::open(temp.path()).expect("store");
    let symbols = store.list_symbols().expect("symbols");
    let a = symbols
        .iter()
        .find(|symbol| symbol.display_name == "old_a")
        .expect("a");
    let b = symbols
        .iter()
        .find(|symbol| symbol.display_name == "old_b")
        .expect("b");
    let mut provenance = a.provenance.clone();
    provenance.source_path = "a.rs".to_owned();
    let edge = EdgeRecord {
        id: GraphId::new("edge:incoming").expect("id"),
        kind: "call".to_owned(),
        from_id: a.id.clone(),
        to_id: Some(b.id.clone()),
        unresolved_target: None,
        provenance: provenance.clone(),
    };
    let reference = ReferenceRecord {
        id: GraphId::new("ref:incoming").expect("id"),
        file_id: a.file_id.clone(),
        kind: "call".to_owned(),
        target_id: Some(b.id.clone()),
        unresolved_target: None,
        span: span(),
        provenance,
    };
    store.save_edge(&edge).expect("incoming edge");
    store
        .save_reference(&reference)
        .expect("incoming reference");
    fs::write(
        temp.path().join("b.rs"),
        "pub fn old_b() {}\npub fn extra_b() {}\n",
    )
    .expect("change target");
    run_index(temp.path()).expect("refresh target");
    assert!(store.list_edges().expect("edges").contains(&edge));
    assert!(store
        .list_references()
        .expect("references")
        .contains(&reference));
    let callers = callers(temp.path(), b.id.as_str(), Some(10)).expect("callers");
    assert!(callers.nodes.iter().any(|symbol| symbol.id == a.id));
}

#[test]
fn interrupted_schema_upgrade_rolls_back_and_can_be_reopened() {
    let temp = tempfile::tempdir().expect("private fixture");
    write_version(temp.path(), "a", false);
    run_index(temp.path()).expect("baseline");
    let connection = Connection::open(temp.path().join(".effigy/graph/graph.db")).expect("sqlite");
    connection.execute_batch("ALTER TABLE file_scan_state DROP COLUMN freshness_key; UPDATE metadata SET value = '2' WHERE key = 'storage_schema_version';").expect("v2 fixture");
    drop(connection);
    crash_refresh(temp.path(), "<schema>", "migrated");
    let store = GraphStore::open(temp.path()).expect("resume interrupted migration");
    assert_eq!(store.storage_schema_version().expect("schema"), 3);
    assert_eq!(store.counts().expect("counts").files, 1);
    assert!(store.file_scan_state_map().expect("states")["a.rs"]
        .freshness_key
        .is_empty());
    assert_eq!(
        status(temp.path()).expect("stale upgrade").stale_paths,
        vec!["a.rs"]
    );
    run_index(temp.path()).expect("resume file upgrade");
    assert!(status(temp.path()).expect("ready").stale_paths.is_empty());
    assert_file_consistent(temp.path(), "a", false);
}
