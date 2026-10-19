//! Cross-process commands against a real refresh paused after a file commit.
use super::*;
use std::collections::BTreeMap;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

type Sentinel = BTreeMap<std::path::PathBuf, (u32, i64, i64, Vec<u8>)>;
fn snapshot(root: &Path) -> Sentinel {
    fn walk(root: &Path, dir: &Path, result: &mut Sentinel) {
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).unwrap();
            result.insert(
                path.strip_prefix(root).unwrap().to_path_buf(),
                (
                    meta.permissions().mode(),
                    meta.mtime(),
                    meta.mtime_nsec(),
                    if meta.is_file() {
                        fs::read(&path).unwrap()
                    } else {
                        Vec::new()
                    },
                ),
            );
            if meta.is_dir() {
                walk(root, &path, result);
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}
fn command(exe: &Path, root: &Path, logs: &Path, args: &[&str]) -> String {
    let out = logs.join("out");
    let err = logs.join("err");
    let mut child = Command::new(exe)
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env_remove("EFFIGY_WORKSPACE_CONTAINER")
        .env_remove("EFFIGY_CONTAINER_HANDOFF")
        .stdout(Stdio::from(fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(fs::File::create(&err).unwrap()))
        .spawn()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if Instant::now() >= end {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("bounded graph read timed out: {args:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    format!(
        "{}{}",
        fs::read_to_string(out).unwrap(),
        fs::read_to_string(err).unwrap()
    )
}
#[test]
#[ignore = "requires the built CLI via test:graph:purity"]
fn status_query_and_doctor_write_nothing_during_and_after_refresh() {
    let owner = tempfile::tempdir().unwrap();
    let root = owner.path().join("repo");
    let logs = owner.path().join("logs");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&logs).unwrap();
    fs::write(
        root.join("effigy.toml"),
        "[tasks.dev]\nrun = 'echo fixture'\n",
    )
    .unwrap();
    for name in ["a", "b"] {
        fs::write(
            root.join(format!("{name}.rs")),
            format!("pub fn old_{name}() {{}}\n"),
        )
        .unwrap();
    }
    run_index(&root).unwrap();
    for name in ["a", "b"] {
        fs::write(
            root.join(format!("{name}.rs")),
            format!("pub fn new_{name}() {{}}\n"),
        )
        .unwrap();
    }
    let exe = std::path::PathBuf::from(
        std::env::var_os("EFFIGY_TEST_EFFIGY_EXECUTABLE").expect("selector supplies built CLI"),
    );
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (resume_tx, resume_rx) = std::sync::mpsc::channel();
    let writer_root = root.clone();
    let writer = std::thread::spawn(move || {
        super::file_transactions::pause_after("a.rs", ready_tx, resume_rx);
        run_index(&writer_root).unwrap();
    });
    ready_rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let check = |refreshing: bool| {
        let before = snapshot(&root);
        for args in [
            vec!["graph", "status", "--json"],
            vec!["graph", "search", "new_a", "--json"],
            vec!["doctor", "--json"],
        ] {
            let output = command(&exe, &root, &logs, &args);
            if args[0] == "doctor" && !refreshing {
                assert!(!output.contains("graph index is not current"), "{output}");
                assert!(output.contains("effigy.doctor.v1"), "{output}");
            } else {
                assert!(
                    output.contains(if refreshing { "refreshing" } else { "ready" }),
                    "{args:?}: {output}"
                );
            }
            assert_eq!(snapshot(&root), before, "read wrote during {args:?}");
            if args[1] == "search" {
                assert!(
                    output.contains("new_a"),
                    "committed progress not queryable: {output}"
                );
            }
        }
    };
    check(true);
    resume_tx.send(()).unwrap();
    writer.join().unwrap();
    check(false);
    assert!(!root.join(".effigy/graph/graph.db-wal").exists());
    assert!(!root.join(".effigy/graph/graph.db-shm").exists());
}

#[test]
fn wal_is_observed_as_stale_and_migrated_only_by_index() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("a.rs"), "pub fn selected_bytes() {}\n").unwrap();
    run_index(temp.path()).unwrap();
    let scope = crate::GraphScope::repo_root(temp.path()).unwrap();
    let writer = Connection::open(scope.paths().db_path).unwrap();
    writer
        .execute_batch("PRAGMA journal_mode=WAL; UPDATE metadata SET value=value;")
        .unwrap();
    let before = snapshot(temp.path());
    let status = crate::status(temp.path()).unwrap();
    assert!(status
        .freshness
        .summary
        .contains("refresh interrupted or in progress"));
    let query = crate::query_search(temp.path(), "selected_bytes", Some(10)).unwrap();
    assert!(query.matches.is_empty());
    assert!(query.freshness.stale);
    assert_eq!(snapshot(temp.path()), before);
    drop(writer);
    // A clean WAL close can remove sidecars while leaving WAL in the DB header.
    let before = snapshot(temp.path());
    assert!(
        crate::query_search(temp.path(), "selected_bytes", Some(10))
            .unwrap()
            .freshness
            .stale
    );
    assert_eq!(snapshot(temp.path()), before);
    run_index(temp.path()).unwrap();
    let query = crate::query_search(temp.path(), "selected_bytes", Some(10)).unwrap();
    assert!(!query.freshness.stale);
    assert!(!query.matches.is_empty());
    assert_eq!(
        GraphStore::open(temp.path())
            .unwrap()
            .journal_mode()
            .unwrap(),
        "delete"
    );
}
