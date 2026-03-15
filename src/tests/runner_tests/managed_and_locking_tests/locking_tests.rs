use super::prelude::{
    assert_live_dev_lock_conflict, assert_output_equals, assert_unlock_invocation_error_case_table,
    assert_unlock_success_case_table, lock_test, run_dev, run_task_with_repo, temp_workspace,
    thread, write_lock_files, write_root_manifest, Duration, ManagedUnlockInvocationErrorCase,
    ManagedUnlockSuccessCase,
};

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

    assert_live_dev_lock_conflict(&root, 120, "task:dev", "effigy unlock task:dev");
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

    write_lock_files(
        &root,
        &[(
            "task-dev.lock",
            r#"{"scope":"task:dev","pid":999999,"started_at_epoch_ms":0}"#,
        )],
    );

    let out = run_dev(&root, &[]).expect("stale lock should be reclaimed");

    assert_output_equals(&out, "");
}

#[test]
fn run_manifest_task_reclaims_stale_lock_from_expired_lease_even_when_pid_is_alive() {
    let _guard = lock_test();
    let root = temp_workspace("lock-stale-expired-lease");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );

    write_lock_files(
        &root,
        &[(
            "task-dev.lock",
            &format!(
                r#"{{"scope":"task:dev","pid":{},"started_at_epoch_ms":0,"heartbeat_at_epoch_ms":0,"hostname":"test-host","workspace_root":"{}"}}"#,
                std::process::id(),
                root.display()
            ),
        )],
    );

    let out = run_dev(&root, &[]).expect("expired lease should be reclaimed");

    assert_output_equals(&out, "");
}

#[test]
fn run_manifest_task_reclaims_invalid_lock_record() {
    let _guard = lock_test();
    let root = temp_workspace("lock-invalid-record-reclaim");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );

    write_lock_files(&root, &[("task-dev.lock", "{invalid-json")]);

    let out = run_dev(&root, &[]).expect("invalid lock record should be reclaimed");

    assert_output_equals(&out, "");
}

#[test]
fn run_manifest_task_builtin_unlock_clears_explicit_scopes() {
    let _guard = lock_test();
    let cases = [ManagedUnlockSuccessCase {
        workspace: "unlock-explicit-scopes",
        args: &["shared:dev-stack", "task:dev"],
        lock_files: &[("shared-dev-stack.lock", "{}"), ("task-dev.lock", "{}")],
        removed_lock_files: &["shared-dev-stack.lock", "task-dev.lock"],
        expected: &["removed: 2"],
    }];

    assert_unlock_success_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_unlock_argument_validation_contract_table() {
    let _guard = lock_test();
    let cases = [
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-requires-scope-or-all",
            args: &[],
            expected: &["`unlock` requires at least one scope (or `--all`)"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-rejects-all-with-scope",
            args: &["--all", "workspace"],
            expected: &["`unlock` accepts either `--all` or explicit scope values, not both"],
        },
    ];

    assert_unlock_invocation_error_case_table(&cases);
}

#[test]
fn different_tasks_do_not_conflict_by_default() {
    let _guard = lock_test();
    let root = temp_workspace("lock-task-default-isolation");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"

[tasks.build]
run = "printf build-ok"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));
    std::thread::sleep(Duration::from_millis(120));

    let _out = run_task_with_repo(&root, "build", &[]).expect("build should not block on dev");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn shared_lock_name_conflicts_across_tasks() {
    let _guard = lock_test();
    let root = temp_workspace("lock-shared-name-conflict");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"
lock = "dev-stack"

[tasks.build]
run = "printf build-ok"
lock = "dev-stack"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));
    std::thread::sleep(Duration::from_millis(120));

    let err = run_task_with_repo(&root, "build", &[]).expect_err("shared lock should conflict");
    super::prelude::assert_lock_conflict(err, "shared:dev-stack", "effigy unlock shared:dev-stack");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}
