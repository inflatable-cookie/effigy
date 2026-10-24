//! Task exclusion through kernel gates (contract 052 §5.4, K1, K2): one gate
//! per resolved task, held exactly while its run lives, named in every
//! conflict, and never unlocked by hand.

use crate::runner::tests::prelude::{
    assert_live_dev_lock_conflict, assert_output_equals, lock_test,
    parse_json_output_with_schema_version, run_dev, run_task_status_from_repo, run_task_with_repo,
    start_held_dev_owner, temp_workspace, wait_for_live_task_owner, write_lock_files,
    write_root_manifest, OwnedThread,
};
use crate::runner::RunnerError;

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

    assert_live_dev_lock_conflict(&root, "task:dev", "effigy tasks status dev");
}

#[test]
fn live_lock_conflict_names_the_holder_and_never_suggests_unlocking() {
    let _guard = lock_test();
    let root = temp_workspace("lock-conflict-names-holder");
    let owner = start_held_dev_owner(&root);

    let err = run_dev(&root, &[]).expect_err("second run should conflict on the gate");
    let RunnerError::TaskLockConflict(details) = &err else {
        panic!("expected a lock conflict, got {err:?}");
    };
    assert_eq!(details.scope, "task:dev");
    assert_eq!(details.holder_pid, Some(std::process::id()));
    assert!(details.holder_run.is_some(), "holder run id: {details:?}");
    let rendered = err.to_string();
    assert!(rendered.contains("already running"), "{rendered}");
    assert!(!rendered.contains("unlock"), "{rendered}");
    let state = crate::runner::task_engine::root_task_state_for_tests(&root, "dev");
    assert_eq!(
        state, "running",
        "the refused run never touches the owner's run"
    );

    owner.release_and_join().expect("first run should complete");
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
    let owner = OwnedThread::spawn(move || run_dev(&root_for_thread, &[]));
    wait_for_live_task_owner(&root, "dev");

    let _out = run_task_with_repo(&root, "build", &[]).expect("build should not block on dev");

    owner.join().expect("first run should complete");
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
    let owner = OwnedThread::spawn(move || run_dev(&root_for_thread, &[]));
    wait_for_live_task_owner(&root, "dev");

    let err = run_task_with_repo(&root, "build", &[]).expect_err("shared lock should conflict");
    crate::runner::tests::prelude::assert_lock_conflict(
        err,
        "shared:dev-stack",
        // The holder is `dev`, not `build`: the inventory names it.
        "effigy tasks status --all",
    );

    owner.join().expect("first run should complete");
}

#[test]
fn a_repository_task_named_tasks_is_never_the_inventory_a_conflict_suggests() {
    let _guard = lock_test();
    let root = temp_workspace("lock-conflict-deferred-tasks");
    write_root_manifest(
        &root,
        r#"[tasks.tasks]
run = "printf repository-tasks-task"
lock = "dev-stack"

[tasks.build]
run = "printf build-ok"
lock = "dev-stack"
"#,
    );

    let root_for_thread = root.clone();
    let owner = OwnedThread::spawn(move || run_task_with_repo(&root_for_thread, "tasks", &[]));
    wait_for_live_task_owner(&root, "tasks");

    // `effigy tasks` runs the repository's own task here, so the conflict must
    // not suggest the built-in inventory (effigy#318).
    let err = run_task_with_repo(&root, "build", &[]).expect_err("shared lock should conflict");
    let RunnerError::TaskLockConflict(details) = &err else {
        panic!("expected a lock conflict, got {err:?}");
    };
    let rendered = err.to_string();
    assert!(
        !rendered.contains("effigy tasks status"),
        "the built-in inventory is routed away here: {rendered}"
    );
    assert!(
        rendered.contains("this repository's own task owns `effigy tasks`"),
        "{rendered}"
    );
    assert_eq!(details.status_command, None);

    owner.join().expect("first run should complete");
}

#[test]
fn colon_namespaced_validation_selectors_do_not_share_locks() {
    let _guard = lock_test();
    let root = temp_workspace("lock-colon-selector-isolation");
    write_root_manifest(
        &root,
        r#"[tasks."validate:activity-routing"]
run = "sleep 1"

[tasks."validate-activity-routing"]
run = "printf other-ok"
"#,
    );

    let root_for_thread = root.clone();
    let owner = OwnedThread::spawn(move || {
        run_task_with_repo(&root_for_thread, "validate:activity-routing", &[])
    });
    wait_for_live_task_owner(&root, "validate:activity-routing");

    let _out = run_task_with_repo(&root, "validate-activity-routing", &[])
        .expect("`a:b` and `a-b` never share a gate");

    owner.join().expect("first run should complete");
}

#[test]
fn lock_wait_timeout_names_live_owner_and_status_reports_it_running() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-timeout-live-owner");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let owner = OwnedThread::spawn(move || run_dev(&root_for_thread, &[]));
    wait_for_live_task_owner(&root, "dev");

    let err = run_dev(&root, &["--lock-wait-ms", "150"])
        .expect_err("live owner should survive a bounded wait");
    let RunnerError::TaskLockConflict(details) = &err else {
        panic!("expected a lock conflict, got {err:?}");
    };
    assert_eq!(details.wait_timeout_ms, Some(150));
    assert!(details
        .details_json
        .as_deref()
        .is_some_and(|json| json.contains("effigy.lock-wait.v2")));

    let status = run_task_status_from_repo(&root, "dev", true);
    let parsed = parse_json_output_with_schema_version(&status, "effigy.tasks-status.v3", 3);
    assert_eq!(parsed["state"], "running");
    assert_eq!(
        parsed["current"]["pid"].as_u64(),
        Some(u64::from(std::process::id()))
    );

    owner.join().expect("first run should complete");
}

#[test]
fn lock_wait_keeps_sequence_owner_on_status() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-timeout-sequence-owner");
    write_root_manifest(
        &root,
        r#"[tasks.validate]
run = [{ task = "work" }]

[tasks.work]
run = "sleep 1"
"#,
    );

    let root_for_thread = root.clone();
    let owner = OwnedThread::spawn(move || run_task_with_repo(&root_for_thread, "validate", &[]));
    wait_for_live_task_owner(&root, "validate");

    let err = run_task_with_repo(&root, "validate", &["--lock-wait-ms", "150"])
        .expect_err("live sequence owner should survive a bounded wait");
    crate::runner::tests::prelude::assert_lock_conflict(
        err,
        "task:validate",
        "effigy tasks status validate",
    );

    let status = run_task_status_from_repo(&root, "validate", true);
    let parsed = parse_json_output_with_schema_version(&status, "effigy.tasks-status.v3", 3);
    assert_eq!(
        parsed["state"], "running",
        "sequence owner missing: {parsed}"
    );

    owner.join().expect("first run should complete");
}

#[test]
fn lock_wait_acquires_after_live_owner_releases() {
    let _guard = lock_test();
    let root = temp_workspace("lock-wait-retry-after-release");
    // The owner's workload ends only when the test releases it, so the waiter
    // starts while the owner is certainly live and inside its workload. What
    // the waiter then waits for is the owner's complete custody settlement,
    // which is what frees the gate; that settle is host-dependent and is not
    // the workload's duration, so the budget is an upper bound rather than a
    // measurement (papercut efb9b1b2). The barrier is the assertion: the
    // waiter must be blocked while the owner lives and must acquire once the
    // owner's settle has finished.
    let owner = start_held_dev_owner(&root);

    let root_for_waiter = root.clone();
    let waiter =
        OwnedThread::spawn(move || run_dev(&root_for_waiter, &["--lock-wait-ms", "30000"]));
    // A waiter is admitted only after the gate frees: while the owner holds
    // it the waiter is still blocked in admission.
    std::thread::sleep(std::time::Duration::from_millis(250));
    assert!(
        !waiter.is_finished(),
        "a waiter acquired a gate its live owner still holds"
    );

    owner.release_and_join().expect("first run should complete");
    let out = waiter.join().expect("waiter should acquire after release");
    assert_output_equals(&out, "");
}

/// v0.14 lease records are never imported or deleted: a dead or corrupt
/// one is ignored and the task runs (contract 052 §11).
#[test]
fn legacy_lease_records_are_ignored_and_left_in_place() {
    let _guard = lock_test();
    let root = temp_workspace("lock-legacy-ignored");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );
    write_lock_files(
        &root,
        &[
            (
                "task-dev.lock",
                r#"{"scope":"task:dev","pid":999999999,"started_at_epoch_ms":1,"heartbeat_at_epoch_ms":1}"#,
            ),
            ("shared-x.lock", "{invalid-json"),
        ],
    );

    run_dev(&root, &[]).expect("a dead legacy holder does not block");
    assert!(root.join(".effigy/locks/task-dev.lock").exists());
    assert!(root.join(".effigy/locks/shared-x.lock").exists());
}

#[test]
fn tasks_unlock_is_retired_with_an_actionable_refusal() {
    let _guard = lock_test();
    let root = temp_workspace("lock-unlock-retired");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );
    let err =
        run_task_with_repo(&root, "tasks", &["unlock", "--all"]).expect_err("unlock is retired");
    let rendered = err.to_string();
    assert!(rendered.contains("was removed in v0.15"), "{rendered}");
    assert!(rendered.contains("effigy tasks status"), "{rendered}");
}

/// Env-schema `exec()` values run only after admission (K3): a run refused
/// by a live holder executes none of them.
#[test]
fn env_schema_exec_runs_only_after_the_gate_admits_the_run() {
    let _guard = lock_test();
    let root = temp_workspace("lock-env-schema-after-admission");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
run = "printf ok"
"#,
    );
    std::fs::write(
        root.join(".env.schema"),
        "# @type=string\nSTAMP=exec('echo ran >> exec.log; echo value')\n",
    )
    .expect("schema");
    let holder = crate::runner::task_engine::TaskRun::hold_for_tests(&root, "dev", None);
    run_dev(&root, &[]).expect_err("the live holder refuses the run");
    assert!(
        !root.join("exec.log").exists(),
        "a refused run must not execute env-schema commands"
    );
    holder.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);

    run_dev(&root, &[]).expect("admitted run resolves its schema");
    assert_eq!(
        std::fs::read_to_string(root.join("exec.log")).expect("exec log"),
        "ran\n"
    );
    assert_eq!(
        crate::runner::task_engine::root_task_state_for_tests(&root, "dev"),
        "not-recorded"
    );
}
