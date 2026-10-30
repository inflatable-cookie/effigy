use crate::runner::entrypoints::run_command_with_context;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{root_task_status_for_tests, TaskRun};
use crate::runner::tests::prelude::{assert_file_text_equals, temp_workspace, write_root_manifest};
use effigy_cli::{Command, TaskInvocation};
use effigy_context::EffigyRuntimeContext;
use effigy_manifest::{with_test_user_config_home, USER_CONFIG_FILE};
use std::fs;

/// Runs `name` as a direct foreground task under a private user-config home,
/// so the host's `~/.effigy/config.toml` cannot move it to durable custody.
fn run_direct_task(name: &str, context: &EffigyRuntimeContext) -> Result<String, RunnerError> {
    let home = temp_workspace(&format!("direct-task-home-{name}"));
    with_test_user_config_home(&home, || {
        run_command_with_context(
            Command::Task(TaskInvocation {
                name: name.to_owned(),
                args: Vec::new(),
            }),
            context,
        )
    })
}

/// A user config home whose `[programs]` block is the only one under test.
fn user_home_with_program(name: &str, program: &str, fallback: &str) -> std::path::PathBuf {
    let home = temp_workspace(name);
    fs::write(
        home.join(USER_CONFIG_FILE),
        format!("[programs.{program}]\nfallback = \"{fallback}\"\n"),
    )
    .expect("write user config");
    home
}

#[test]
fn direct_foreground_dispatch_stays_in_memory_when_configured_program_is_inactive() {
    // `sh` is on PATH and not forced, so the block puts no shim on this run.
    let home = user_home_with_program("task-status-program-inactive-home", "sh", "true");
    let root = temp_workspace("task-status-program-inactive");
    write_root_manifest(&root, "[tasks.echo]\nrun = \"printf ok\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    with_test_user_config_home(&home, || {
        run_command_with_context(
            Command::Task(TaskInvocation {
                name: "echo".to_owned(),
                args: Vec::new(),
            }),
            &context,
        )
        .expect("direct task")
    });

    let status = root_task_status_for_tests(&root, "echo");
    assert_eq!(status.state, "not-recorded");
    assert!(status.latest.is_none());
    assert!(status.history.is_empty());
}

#[test]
fn direct_foreground_dispatch_records_durably_when_configured_program_activates() {
    // The program is absent from PATH, so its shim activates and the run must
    // be durable: a shim cannot ride on foreground memory custody.
    let home = user_home_with_program(
        "task-status-program-active-home",
        "effigy-no-such-program-491",
        "sh",
    );
    let root = temp_workspace("task-status-program-active");
    write_root_manifest(&root, "[tasks.echo]\nrun = \"printf ok\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    with_test_user_config_home(&home, || {
        run_command_with_context(
            Command::Task(TaskInvocation {
                name: "echo".to_owned(),
                args: Vec::new(),
            }),
            &context,
        )
        .expect("direct task")
    });

    let status = root_task_status_for_tests(&root, "echo");
    let latest = status.latest.expect("durable run recorded");
    assert_eq!(latest.exit_code, Some(0));
}

#[test]
fn direct_foreground_dispatch_succeeds_without_recording_runtime_history() {
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

    run_direct_task("echo", &context).expect("direct task");

    assert_file_text_equals(&marker, "ok");
    let status = root_task_status_for_tests(&root, "echo");
    assert_eq!(status.state, "not-recorded");
    assert!(status.latest.is_none());
    assert!(status.history.is_empty());
    assert!(!root.join(".effigy/state/kernel.db").exists());
    assert!(status.current.is_none());
    assert!(!root.join(".effigy/reports").exists());
    assert!(!root.join(".effigy/runtime/tasks").exists());
    assert!(!root.join(".effigy/locks").exists());
}

#[test]
fn direct_foreground_dispatch_reports_failure_without_recording_history() {
    let root = temp_workspace("task-status-failed");
    write_root_manifest(&root, "[tasks.fail]\nrun = \"sh -c 'exit 7'\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_direct_task("fail", &context).expect_err("task should fail");
    match error {
        RunnerError::TaskCommandFailure { code, .. } => assert_eq!(code, Some(7)),
        other => panic!("unexpected error: {other}"),
    }

    let status = root_task_status_for_tests(&root, "fail");
    assert_eq!(status.state, "not-recorded");
    assert!(status.latest.is_none());
    assert!(status.history.is_empty());
}

#[test]
fn direct_foreground_dispatch_reports_cancellation_without_recording_history() {
    let root = temp_workspace("task-status-cancelled");
    write_root_manifest(&root, "[tasks.stop]\nrun = \"sh -c 'exit 130'\"\n");
    let context = EffigyRuntimeContext::capture(Some(root.clone()), None).expect("runtime context");

    let error = run_direct_task("stop", &context).expect_err("task should cancel");
    match error {
        RunnerError::TaskCommandFailure { code, .. } => assert_eq!(code, Some(130)),
        other => panic!("unexpected error: {other}"),
    }

    let status = root_task_status_for_tests(&root, "stop");
    assert_eq!(status.state, "not-recorded");
    assert!(status.latest.is_none());
    assert!(status.history.is_empty());
}

#[test]
fn direct_task_dispatch_refused_by_a_live_holder_records_no_run_of_its_own() {
    let root = temp_workspace("task-status-blocked");
    write_root_manifest(&root, "[tasks.echo]\nrun = \"printf blocked\"\n");
    let holder = TaskRun::hold_for_tests(&root, "echo", None);
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

    let status = root_task_status_for_tests(&root, "echo");
    assert_eq!(status.state, "running", "the holder's run is untouched");
    assert_eq!(status.history.len(), 1, "a refused admission writes no run");
    holder.close(effigy_kernel::outcome::WorkloadOutcome::Succeeded);
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
    write_root_manifest(&root, "[tasks.build]\nrun = \"printf ok\"\n");
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
