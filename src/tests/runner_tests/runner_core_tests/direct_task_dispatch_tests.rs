use crate::runner::entrypoints::run_command_with_context;
use crate::runner::error::RunnerError;
use crate::runner::tests::prelude::{assert_file_text_equals, temp_workspace, write_root_manifest};
use effigy_builtin::LockScope;
use effigy_cli::{Command, TaskInvocation};
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{TaskStatusCompletedRecord, TaskStatusState};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn direct_task_dispatch_runs_through_execution_request_boundary() {
    let root = temp_workspace("direct-task-execution-request");
    let marker = root.join("direct-task.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.echo]\nrun = \"printf direct-request > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    run_command_with_context(
        Command::Task(TaskInvocation {
            name: "echo".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect("direct task");

    assert_file_text_equals(&marker, "direct-request");
}

#[test]
fn direct_task_dispatch_writes_succeeded_task_status_record_and_clears_active_record() {
    let root = temp_workspace("task-status-success");
    let marker = root.join("status-success.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.echo]\nrun = \"printf ok > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    run_command_with_context(
        Command::Task(TaskInvocation {
            name: "echo".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect("direct task");

    assert_file_text_equals(&marker, "ok");
    let record = latest_task_status_record(&root);
    assert_eq!(record.state, TaskStatusState::Succeeded);
    assert_eq!(record.identity.resolved_selector, "echo");
    assert!(record.latest_report_path.ends_with("/latest.json"));
    assert_active_task_status_dir_empty(&root);
}

#[test]
fn direct_task_dispatch_writes_failed_task_status_record() {
    let root = temp_workspace("task-status-failed");
    write_root_manifest(&root, "[tasks.fail]\nrun = \"sh -c 'exit 7'\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "fail".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect_err("task should fail");
    match error {
        RunnerError::TaskCommandFailure { code, .. } => assert_eq!(code, Some(7)),
        other => panic!("unexpected error: {other}"),
    }

    let record = latest_task_status_record(&root);
    assert_eq!(record.state, TaskStatusState::Failed);
    assert_eq!(
        record.outcome.error_family.as_deref(),
        Some("task-command-failure")
    );
    assert_eq!(record.outcome.error_code.as_deref(), Some("7"));
    assert_active_task_status_dir_empty(&root);
}

#[test]
fn direct_task_dispatch_writes_cancelled_task_status_record_for_exit_130() {
    let root = temp_workspace("task-status-cancelled");
    write_root_manifest(&root, "[tasks.stop]\nrun = \"sh -c 'exit 130'\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "stop".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect_err("task should cancel");
    match error {
        RunnerError::TaskCommandFailure { code, .. } => assert_eq!(code, Some(130)),
        other => panic!("unexpected error: {other}"),
    }

    let record = latest_task_status_record(&root);
    assert_eq!(record.state, TaskStatusState::Cancelled);
    assert_eq!(record.outcome.error_code.as_deref(), Some("130"));
    assert_active_task_status_dir_empty(&root);
}

#[test]
fn direct_task_dispatch_writes_blocked_task_status_record_for_lock_conflict() {
    let root = temp_workspace("task-status-blocked");
    write_root_manifest(&root, "[tasks.echo]\nrun = \"printf blocked\"\n");
    seed_live_lock(&root, LockScope::Task("echo".to_owned()));
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "echo".to_owned(),
            args: Vec::new(),
        }),
        &context,
    )
    .expect_err("task should block");
    match error {
        RunnerError::TaskLockConflict(_) => {}
        other => panic!("unexpected error: {other}"),
    }

    let record = latest_task_status_record(&root);
    assert_eq!(record.state, TaskStatusState::Blocked);
    assert_eq!(
        record.outcome.error_family.as_deref(),
        Some("task-lock-conflict")
    );
    assert_active_task_status_dir_empty(&root);
}

fn latest_task_status_record(root: &std::path::Path) -> TaskStatusCompletedRecord {
    let reports_root = root.join(".effigy/reports/tasks");
    let mut latest_paths = fs::read_dir(&reports_root)
        .expect("read task status reports")
        .map(|entry| entry.expect("report dir").path().join("latest.json"))
        .collect::<Vec<_>>();
    latest_paths.sort();
    assert_eq!(latest_paths.len(), 1, "expected one task-status report");
    let latest = fs::read_to_string(&latest_paths[0]).expect("read latest record");
    serde_json::from_str(&latest).expect("parse latest record")
}

fn assert_active_task_status_dir_empty(root: &std::path::Path) {
    let active_root = root.join(".effigy/runtime/tasks/active");
    let entries = fs::read_dir(&active_root)
        .expect("read active task-status dir")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect active entries");
    assert!(entries.is_empty(), "expected active dir to be empty");
}

fn seed_live_lock(root: &std::path::Path, scope: LockScope) {
    let locks_root = root.join(".effigy/locks");
    fs::create_dir_all(&locks_root).expect("create locks root");
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("unix time")
        .as_millis();
    let record = serde_json::json!({
        "scope": scope.label(),
        "pid": std::process::id(),
        "started_at_epoch_ms": now,
        "heartbeat_at_epoch_ms": now,
        "workspace_root": root.display().to_string(),
    });
    fs::write(
        locks_root.join(scope.file_name()),
        serde_json::to_vec(&record).expect("encode lock"),
    )
    .expect("write lock");
}
