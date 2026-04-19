use std::path::Path;

use effigy_core::widgets::{KeyValue, SummaryCounts};
use effigy_process::ProcessSupervisor;
use effigy_tui::multiprocess::{run_multiprocess_tui, MultiProcessTuiOptions};
use effigy_ui::Renderer;

use super::render_support::{managed_process_specs, write_managed_overview};
use crate::ManagedError;
use crate::ManagedTaskPlan;
use crate::{render_utf8, text_renderer};

#[path = "runtime/policy.rs"]
mod policy;
#[path = "runtime/stream.rs"]
mod stream;

pub fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, ManagedError> {
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        gateway_auto_start: _,
        readiness: _,
        ..
    } = plan;
    let specs = managed_process_specs(processes);
    let outcome = run_multiprocess_tui(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
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
        outcome.non_zero_exits,
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
            "ready-message",
            plan.readiness
                .ready_message
                .clone()
                .unwrap_or_else(|| "disabled".to_owned()),
        ),
    ]
}
