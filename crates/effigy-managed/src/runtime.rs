use std::path::Path;

use effigy_core::widgets::{KeyValue, SummaryCounts};
use effigy_process::ProcessSupervisor;
use effigy_tui::multiprocess::{
    run_multiprocess_tui_with_control, MultiProcessTuiControl, MultiProcessTuiOptions,
};
use effigy_ui::Renderer;

use super::render_support::{managed_process_specs, write_managed_overview};
use crate::ManagedError;
use crate::ManagedTaskPlan;
use crate::{render_utf8, text_renderer};

#[path = "runtime/headless.rs"]
mod headless;
#[path = "runtime/policy.rs"]
mod policy;
#[path = "runtime/redaction.rs"]
mod redaction;
#[path = "runtime/stream.rs"]
mod stream;

pub use headless::{
    clear_managed_run_failure, exec_group_file_path, failed_managed_run_token,
    failed_managed_run_tokens, live_headless_sessions, managed_headless_logs,
    managed_headless_state_readable, managed_headless_status, managed_headless_stop,
    managed_owner_is_running, managed_run_settle_error, record_managed_run_settle,
    run_managed_task_headless, session_runtime_dir, LiveHeadlessSession,
    ManagedHeadlessStopOutcome, LIFECYCLE_CLEANUP_TIMEOUT_SECS, MANAGED_SETTLE_BUDGET,
};

pub fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
    settle: Option<&crate::ManagedRunSettle<'_>>,
) -> Result<String, ManagedError> {
    let session = headless::TuiSessionControl::new(task_name, repo_root, &plan)?;
    let owns_environment = plan
        .processes
        .iter()
        .any(|process| process.role == crate::ManagedProcessRole::Lifecycle);
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        ..
    } = plan;
    let specs = managed_process_specs(processes);
    let poll = |supervisor: &ProcessSupervisor| session.poll(supervisor);
    let before_shutdown = |supervisor: &ProcessSupervisor| {
        session.poll(supervisor)?;
        let result = session.end_exec_groups(settle);
        // The runner's final down is the backstop for an owned environment.
        if owns_environment {
            Ok(())
        } else {
            result
        }
    };
    let after_shutdown = |supervisor: &ProcessSupervisor| session.finish(supervisor);
    let control = MultiProcessTuiControl {
        poll: &poll,
        before_shutdown: &before_shutdown,
        after_shutdown: &after_shutdown,
    };
    let outcome = run_multiprocess_tui_with_control(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
        Some(&control),
    )
    .map_err(|error| {
        ManagedError::Ui(format!(
            "managed tui runtime failed for task `{task_name}`: {error}"
        ))
    })?;
    policy::enforce_non_zero_exit_policy(
        task_name,
        &profile,
        fail_on_non_zero,
        if outcome.stopped_by_user {
            Vec::new()
        } else {
            outcome.non_zero_exits
        },
    )?;
    Ok(String::new())
}

pub fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, ManagedError> {
    let shutdown_on_exit_processes = plan
        .processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let readiness_fields = managed_runtime_readiness_fields(&plan);
    let secret_values = managed_secret_values(&plan);
    let specs = managed_process_specs(plan.processes.iter().cloned());
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;

    let mut renderer = text_renderer();
    write_managed_overview(
        &mut renderer,
        "Managed Task Runtime",
        task_name,
        &plan,
        Vec::new(),
        readiness_fields,
        &["Running managed profile in temporary stream mode."],
    )?;
    let non_zero_exits = stream::collect_stream_non_zero_exits(
        &supervisor,
        expected,
        &shutdown_on_exit_processes,
        &secret_values,
        &mut renderer,
    )?;

    supervisor.terminate_all();
    policy::enforce_non_zero_exit_policy(
        task_name,
        &plan.profile,
        plan.fail_on_non_zero,
        non_zero_exits,
    )?;
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: expected,
        warn: 1,
        err: 0,
    })?;
    render_utf8(renderer.into_inner())
}

fn managed_secret_values(plan: &ManagedTaskPlan) -> Vec<String> {
    plan.processes
        .iter()
        .flat_map(|process| process.secret_env.values())
        .filter(|value| !value.is_empty())
        .cloned()
        .collect()
}

fn managed_runtime_readiness_fields(plan: &ManagedTaskPlan) -> Vec<KeyValue> {
    vec![
        KeyValue::new(
            "gateway-auto-start",
            if plan.gateway_auto_start {
                "enabled"
            } else {
                "disabled"
            },
        ),
        KeyValue::new(
            "readiness-wait",
            if plan.readiness.health_wait {
                "enabled"
            } else {
                "disabled"
            },
        ),
        KeyValue::new(
            "readiness-timeout",
            format!("{}s", plan.readiness.timeout_secs),
        ),
        KeyValue::new(
            "ready-message",
            plan.readiness
                .ready_message
                .clone()
                .unwrap_or_else(|| "disabled".to_owned()),
        ),
    ]
}
