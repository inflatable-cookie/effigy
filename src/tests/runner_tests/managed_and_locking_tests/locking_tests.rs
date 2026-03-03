use super::prelude::*;

#[test]
fn run_manifest_task_rejects_live_lock_conflict() {
    let _guard = lock_test();
    let root = temp_workspace("lock-conflict-live");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));

    std::thread::sleep(Duration::from_millis(120));

    let err = run_dev(&root, &[]).expect_err("second run should conflict on lock");
    assert_lock_conflict(err, "workspace", "effigy unlock workspace");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn run_manifest_task_reclaims_stale_lock_from_dead_pid() {
    let _guard = lock_test();
    let root = temp_workspace("lock-stale-reclaim");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );

    let locks_dir = root.join(".effigy/locks");
    fs::create_dir_all(&locks_dir).expect("create locks dir");
    fs::write(
        locks_dir.join("workspace.lock"),
        r#"{"scope":"workspace","pid":999999,"started_at_epoch_ms":0}"#,
    )
    .expect("write stale lock");

    let out = run_dev(&root, &[]).expect("stale lock should be reclaimed");

    assert_eq!(out, "");
}

#[test]
fn run_manifest_task_builtin_unlock_clears_explicit_scopes() {
    let _guard = lock_test();
    let root = temp_workspace("unlock-explicit-scopes");
    fs::create_dir_all(root.join(".effigy/locks")).expect("mkdir locks");
    fs::write(root.join(".effigy/locks/workspace.lock"), "{}").expect("write workspace lock");
    fs::write(root.join(".effigy/locks/task-dev.lock"), "{}").expect("write task lock");

    let out = run_unlock_with_repo(&root, &["workspace", "task:dev"]).expect("unlock should run");
    assert_contains_all(&out, &["removed: 2"]);
    assert!(!root.join(".effigy/locks/workspace.lock").exists());
    assert!(!root.join(".effigy/locks/task-dev.lock").exists());
}
