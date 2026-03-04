use super::prelude::*;

#[test]
fn run_manifest_task_builtin_watch_once_executes_target_task() {
    let root = temp_workspace("builtin-watch-once-exec");
    let marker = root.join("watch-once.log");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.build]\nrun = \"printf watched > '{}'\"\n",
            marker.display()
        ),
    );

    let out = run_builtin_ok(root, "watch", &["--owner", "effigy", "--once", "build"]);
    assert_contains_all(&out, &["watch complete after 1 run(s)."]);
    assert!(marker.exists(), "watch --once should execute the target");
}

#[test]
fn run_manifest_task_builtin_watch_rejects_concurrent_watch_owner_for_same_target() {
    let _guard = lock_test();
    let root = temp_workspace("builtin-watch-lock-conflict");
    write_build_task_manifest(&root, "sleep 2");

    let root_for_thread = root.to_path_buf();
    let join = thread::spawn(move || {
        run_task(
            &root_for_thread,
            "watch",
            &["--owner", "effigy", "--once", "build"],
        )
    });

    let watch_lock = root.join(".effigy/locks/task-watch-build.lock");
    wait_for_path_exists(
        &watch_lock,
        Duration::from_secs(5),
        "watch lock for owner=effigy target=build",
    );

    let err = run_task(&root, "watch", &["--owner", "effigy", "--once", "build"])
        .expect_err("second watch owner should conflict on watch scope lock");
    assert_lock_conflict(err, "task:watch:build", "effigy unlock task:watch:build");

    let first = join.join().expect("thread join");
    first.expect("first watch should complete");
}
