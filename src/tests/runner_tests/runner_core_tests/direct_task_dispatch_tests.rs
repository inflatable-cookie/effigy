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

#[test]
fn selector_plan_does_not_start_the_task_process() {
    let root = temp_workspace("selector-plan-no-process");
    let marker = root.join("plan-must-not-run.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.slow]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let output = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "slow".to_owned(),
            args: vec!["--plan".to_owned()],
        }),
        &context,
    )
    .expect("selector plan");

    assert!(
        !marker.exists(),
        "plan must not start the task process: {}",
        marker.display()
    );
    assert!(
        output.contains("Selector: slow"),
        "plain plan should name the selector: {output}"
    );
    assert!(
        output.contains("Command:"),
        "plain plan should include command shape: {output}"
    );
    assert!(
        output.contains(&format!("printf ran > '{}'", marker.display())),
        "plain plan should include the resolved command: {output}"
    );
}

#[test]
fn selector_plan_json_reports_command_shape_without_executing() {
    let root = temp_workspace("selector-plan-json-no-process");
    let marker = root.join("plan-json-must-not-run.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.probe]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let output = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "probe".to_owned(),
            args: vec!["--json".to_owned(), "--plan".to_owned()],
        }),
        &context,
    )
    .expect("selector plan json");

    assert!(
        !marker.exists(),
        "json plan must not start the task process: {}",
        marker.display()
    );
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("plan json");
    assert_eq!(parsed["schema"], "effigy.task.plan.v1");
    assert_eq!(parsed["schema_version"], 1);
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["executed"], false);
    assert_eq!(parsed["task"], "probe");
    assert_eq!(parsed["selector"], "probe");
    assert!(
        parsed["command"]
            .as_str()
            .is_some_and(|command| command.contains("printf ran")),
        "plan command should match the task body: {parsed}"
    );
    assert!(parsed["catalog"]["root"].is_string());
    assert!(parsed["catalog"]["manifest"].is_string());
}

#[test]
fn json_selector_without_plan_still_executes() {
    let root = temp_workspace("selector-json-still-executes");
    let marker = root.join("json-should-run.out");
    write_root_manifest(
        &root,
        &format!(
            "[tasks.probe]\nrun = \"printf ran > '{}'\"\n",
            marker.display()
        ),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let output = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "probe".to_owned(),
            args: vec!["--json".to_owned()],
        }),
        &context,
    )
    .expect("json execution");

    assert_file_text_equals(&marker, "ran");
    let parsed: serde_json::Value = serde_json::from_str(&output).expect("run json");
    assert_eq!(parsed["schema"], "effigy.task.run.v1");
    assert_eq!(parsed["ok"], true);
    assert_eq!(parsed["task"], "probe");
}

#[test]
fn selector_plan_does_not_run_deferral_fallback() {
    let root = temp_workspace("selector-plan-no-defer");
    let marker = root.join("defer-must-not-run.out");
    write_root_manifest(
        &root,
        &format!("[defer]\nrun = \"printf ran > '{}'\"\n", marker.display()),
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "missing-task".to_owned(),
            args: vec!["--plan".to_owned()],
        }),
        &context,
    )
    .expect_err("missing selector plan should not defer");

    assert!(
        !marker.exists(),
        "plan must not start the deferral process: {}",
        marker.display()
    );
    assert!(
        matches!(error, RunnerError::TaskNotFoundAny { .. }),
        "expected catalog miss, got {error}"
    );
}

#[test]
fn selector_plan_renders_managed_tui_process_shape() {
    let root = temp_workspace("selector-plan-managed-tui");
    write_root_manifest(
        &root,
        r#"[tasks.dev]
mode = "tui"
concurrent = [
  { name = "api", run = "cargo run -p api", start = 1, tab = 1 },
  { name = "web", run = "vite dev", start = 2, tab = 2 }
]
"#,
    );
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let output = run_command_with_context(
        Command::Task(TaskInvocation {
            name: "dev".to_owned(),
            args: vec!["--plan".to_owned()],
        }),
        &context,
    )
    .expect("managed tui plan");

    assert!(output.contains("Selector: dev"), "{output}");
    assert!(
        output.contains("api: cargo run -p api"),
        "tui plan should name process commands: {output}"
    );
    assert!(output.contains("web: vite dev"), "{output}");
}

#[test]
fn selector_plan_does_not_resolve_env_schema_exec() {
    let root = temp_workspace("selector-plan-no-env-exec");
    let marker = root.join("env-exec-must-not-run.out");
    write_root_manifest(
        &root,
        "[tasks.build]\nrun = \"printf ok\"\n",
    );
    fs::write(
        root.join(".env.schema"),
        format!("SIDE=exec('printf ran > {}')\n", marker.display()),
    )
    .expect("write env schema");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    run_command_with_context(
        Command::Task(TaskInvocation {
            name: "build".to_owned(),
            args: vec!["--plan".to_owned()],
        }),
        &context,
    )
    .expect("selector plan");

    assert!(
        !marker.exists(),
        "plan must not run env-schema exec(): {}",
        marker.display()
    );
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
