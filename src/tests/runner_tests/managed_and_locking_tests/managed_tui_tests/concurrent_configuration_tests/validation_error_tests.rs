use super::prelude::*;

#[test]
fn run_manifest_task_managed_tui_rejects_concurrent_entry_with_both_task_and_run() {
    let root = temp_workspace("managed-concurrent-invalid-entry");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { task = "api", run = "printf oops", start = 1, tab = 1 }
]

[tasks.api]
run = "printf api"
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", Some("either `task` or `run`"));
}

#[test]
fn run_manifest_task_managed_tui_errors_when_concurrent_entry_missing_task_and_run() {
    let root = temp_workspace("managed-tab-order-invalid");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "jobs" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid concurrent entry should fail");
    assert_managed_process_invalid_definition(
        err,
        "dev",
        "jobs",
        Some("missing both `task` and `run`"),
    );
}

#[test]
fn run_manifest_task_managed_tui_errors_when_process_has_run_and_task() {
    let root = temp_workspace("managed-invalid-process-def");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [{ name = "api", run = "printf api", task = "api" }]
"#,
    );

    let err = run_dev_with_repo(&root, &[]).expect_err("invalid process definition should fail");
    assert_managed_process_invalid_definition(err, "dev", "api", None);
}
