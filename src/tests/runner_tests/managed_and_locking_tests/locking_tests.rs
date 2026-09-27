use crate::runner::tests::prelude::{
    assert_live_dev_lock_conflict, assert_output_equals, assert_unlock_invocation_error_case_table,
    assert_unlock_success_case_table, lock_test, parse_json_output_with_schema_version, run_dev,
    run_task_status_from_repo, run_task_with_repo, temp_workspace, thread, write_lock_files,
    write_root_manifest, Duration, ManagedUnlockInvocationErrorCase, ManagedUnlockSuccessCase,
};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

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

    assert_live_dev_lock_conflict(&root, 120, "task:dev", "effigy tasks unlock task:dev");
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

#[cfg(unix)]
#[test]
fn run_manifest_task_reclaims_lock_from_reused_live_pid_that_is_not_effigy() {
    let _guard = lock_test();
    let root = temp_workspace("lock-stale-live-non-effigy");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );

    let mut child = Command::new("sleep")
        .arg("5")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn sleep");

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_millis();
    write_lock_files(
        &root,
        &[(
            "task-dev.lock",
            &format!(
                r#"{{"scope":"task:dev","pid":{},"started_at_epoch_ms":{},"heartbeat_at_epoch_ms":{},"hostname":"test-host","workspace_root":"{}"}}"#,
                child.id(),
                now,
                now,
                root.display()
            ),
        )],
    );

    let out = run_dev(&root, &[]).expect("non-effigy live pid lock should be reclaimed");
    assert_output_equals(&out, "");

    let _ = child.kill();
    let _ = child.wait();
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
    let cases = [
        ManagedUnlockSuccessCase {
            workspace: "unlock-explicit-task-scope",
            args: &["task:dev"],
            lock_files: &[("task-dev.lock", "{}")],
            removed_lock_files: &["task-dev.lock"],
            expected: &["removed: 1"],
        },
        ManagedUnlockSuccessCase {
            workspace: "unlock-selector-form-task-scope",
            args: &["validate:activity-routing"],
            lock_files: &[("task-validate-activity-routing.lock", "{}")],
            removed_lock_files: &["task-validate-activity-routing.lock"],
            expected: &["removed: 1"],
        },
        ManagedUnlockSuccessCase {
            workspace: "unlock-explicit-profile-scope",
            args: &["profile:watch/test"],
            lock_files: &[("profile-watch-test.lock", "{}")],
            removed_lock_files: &["profile-watch-test.lock"],
            expected: &["removed: 1"],
        },
        ManagedUnlockSuccessCase {
            workspace: "unlock-explicit-broad-scopes-yes",
            args: &["shared:dev-stack", "task:dev", "--yes"],
            lock_files: &[("shared-dev-stack.lock", "{}"), ("task-dev.lock", "{}")],
            removed_lock_files: &["shared-dev-stack.lock", "task-dev.lock"],
            expected: &["removed: 2"],
        },
        ManagedUnlockSuccessCase {
            workspace: "unlock-all-yes",
            args: &["--all", "--yes"],
            lock_files: &[("shared-dev-stack.lock", "{}"), ("task-dev.lock", "{}")],
            removed_lock_files: &["shared-dev-stack.lock", "task-dev.lock"],
            expected: &["mode: all", "removed: 2"],
        },
    ];

    assert_unlock_success_case_table(&cases);
}

#[test]
fn run_manifest_task_builtin_unlock_argument_validation_contract_table() {
    let _guard = lock_test();
    let cases = [
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-requires-scope-or-all",
            args: &[],
            expected: &["`tasks unlock` requires at least one scope (or `--all`)"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-rejects-all-with-scope",
            args: &["--all", "workspace"],
            expected: &["`tasks unlock` accepts either `--all` or explicit scope values, not both"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-all-requires-confirmation",
            args: &["--all"],
            expected: &["requires confirmation", "--yes"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-shared-requires-confirmation",
            args: &["shared:dev-stack"],
            expected: &["requires confirmation", "--yes"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-multiple-scopes-requires-confirmation",
            args: &["task:dev", "profile:watch/test"],
            expected: &["requires confirmation", "--yes"],
        },
        ManagedUnlockInvocationErrorCase {
            workspace: "unlock-incomplete-profile-explains-selector-form",
            args: &["profile:foo"],
            expected: &["profile:<task>/<profile>", "task:validate:activity-routing"],
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
    crate::runner::tests::prelude::assert_lock_conflict(
        err,
        "shared:dev-stack",
        "effigy tasks unlock shared:dev-stack",
    );

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn colon_namespaced_validation_selectors_do_not_share_locks() {
    let _guard = lock_test();
    let root = temp_workspace("lock-colon-selector-isolation");
    write_root_manifest(
        &root,
        r#"[tasks."validate:activity-routing"]
run = "sleep 1"

[tasks."validate:other"]
run = "printf other-ok"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || {
        run_task_with_repo(&root_for_thread, "validate:activity-routing", &[])
    });
    std::thread::sleep(Duration::from_millis(120));

    let _out = run_task_with_repo(&root, "validate:other", &[])
        .expect("independent validation selector should not block");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn lock_wait_timeout_names_live_owner_and_status_path() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-timeout-live-owner");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));
    std::thread::sleep(Duration::from_millis(120));

    let err = run_dev(&root, &["--lock-wait-ms", "150"])
        .expect_err("live owner should survive a bounded wait");
    crate::runner::tests::prelude::assert_lock_conflict(err, "task:dev", "effigy tasks status dev");

    let status = run_task_status_from_repo(&root, "dev", true);
    let parsed = parse_json_output_with_schema_version(&status, "effigy.tasks-status.v1", 1);
    assert_eq!(parsed["state"], "running");
    assert!(parsed["active"].is_object(), "live owner missing: {parsed}");
    assert!(parsed["active"]["owner_pid"].as_u64().is_some());

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn lock_wait_acquires_after_live_owner_releases() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-retry-after-release");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 0.2"
"#,
    );

    let root_for_thread = root.clone();
    let join = thread::spawn(move || run_dev(&root_for_thread, &[]));
    std::thread::sleep(Duration::from_millis(80));

    let out =
        run_dev(&root, &["--lock-wait-ms", "2000"]).expect("waiter should acquire after release");
    assert_output_equals(&out, "");

    join.join()
        .expect("thread join")
        .expect("first run should complete");
}

#[test]
fn lock_wait_reclaims_stopped_owner_during_wait() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-stopped-owner");
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

    let out = run_dev(&root, &["--lock-wait-ms", "250"])
        .expect("stopped owner should be reclaimed during wait");
    assert_output_equals(&out, "");
}
