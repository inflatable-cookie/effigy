use super::prelude::{
    assert_live_dev_lock_conflict, assert_output_equals, assert_unlock_invocation_error_case_table,
    assert_unlock_success_case_table, lock_test, run_dev, temp_workspace, write_lock_files,
    write_root_manifest, ManagedUnlockInvocationErrorCase, ManagedUnlockSuccessCase,
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

    assert_live_dev_lock_conflict(&root, 120, "workspace", "effigy unlock workspace");
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
            "workspace.lock",
            r#"{"scope":"workspace","pid":999999,"started_at_epoch_ms":0}"#,
        )],
    );

    let out = run_dev(&root, &[]).expect("stale lock should be reclaimed");

    assert_output_equals(&out, "");
}

#[test]
fn run_manifest_task_builtin_unlock_clears_explicit_scopes() {
    let _guard = lock_test();
    let cases = [ManagedUnlockSuccessCase {
        workspace: "unlock-explicit-scopes",
        args: &["workspace", "task:dev"],
        lock_files: &[("workspace.lock", "{}"), ("task-dev.lock", "{}")],
        removed_lock_files: &["workspace.lock", "task-dev.lock"],
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
