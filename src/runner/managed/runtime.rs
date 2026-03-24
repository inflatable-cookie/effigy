use std::path::Path;

use crate::process_manager::ProcessSupervisor;
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::ui::{Renderer, SummaryCounts};

use super::super::model::managed::ManagedTaskPlan;
use super::super::render::{render_utf8, text_renderer};
use super::render_support::{managed_process_specs, write_managed_overview};
use crate::runner::error::RunnerError;

#[path = "runtime/policy.rs"]
mod policy;
#[path = "runtime/stream.rs"]
mod stream;

pub(super) fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
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
        RunnerError::Ui(format!(
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

pub(super) fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let shutdown_on_exit_processes = plan
        .processes
        .iter()
        .filter(|process| process.shutdown_on_exit)
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
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
        Vec::new(),
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
