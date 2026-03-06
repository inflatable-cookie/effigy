use std::path::Path;
use std::time::Duration;

use crate::process_manager::{ProcessEventKind, ProcessSupervisor};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::ui::{NoticeLevel, Renderer, SummaryCounts};

use super::super::render::{render_utf8, text_renderer};
use super::super::{ManagedTaskPlan, RunnerError};
use super::render_support::{managed_process_specs, write_managed_overview};

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
    if fail_on_non_zero && !outcome.non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile,
            processes: outcome.non_zero_exits,
        });
    }
    Ok(String::new())
}

pub(super) fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
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

    let mut exit_count = 0usize;
    let mut drained_after_exit = 0usize;
    let mut non_zero_exits = Vec::<(String, String)>::new();
    while exit_count < expected || drained_after_exit < 3 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if exit_count >= expected {
                drained_after_exit = 0;
            }
            match event.kind {
                ProcessEventKind::Stdout => {
                    renderer.text(&format!("[{}] {}", event.process, event.payload))?;
                }
                ProcessEventKind::Stderr => {
                    renderer.text(&format!("[{} stderr] {}", event.process, event.payload))?;
                }
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => {
                    exit_count += 1;
                    if event.payload != "exit=0" {
                        non_zero_exits.push((event.process.clone(), event.payload.clone()));
                    }
                    renderer.notice(
                        NoticeLevel::Info,
                        &format!("process `{}` {}", event.process, event.payload),
                    )?;
                }
            }
        } else if exit_count >= expected {
            drained_after_exit += 1;
        }
    }

    supervisor.terminate_all();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if plan.fail_on_non_zero && !non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile: plan.profile,
            processes: non_zero_exits,
        });
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: expected,
        warn: 1,
        err: 0,
    })?;
    render_utf8(renderer.into_inner())
}
