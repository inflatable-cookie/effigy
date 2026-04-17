use effigy_cli::TaskInvocation;

use super::super::preflight::ExecutionPreflight;
use super::super::selection::result;
use super::super::selection::SelectionResolution;
use crate::runner::builtin_ports::RunnerBuiltinPorts;
use crate::runner::deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use crate::runner::error::RunnerError;
use effigy_builtin::try_run_builtin_task;
use effigy_tasks::TaskSelector;

pub(super) fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    if let Some(removed_builtin_error) = removed_builtin_invocation_error(&preflight.selector) {
        return Err(removed_builtin_error);
    }
    if let Some(output) = resolve_builtin_selection_output(task, preflight)? {
        return Ok(result::output(output));
    }
    if let Some(output) = resolve_deferred_selection_output(task, preflight, &error)? {
        return Ok(result::output(output));
    }
    Err(error)
}

fn resolve_builtin_selection_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
) -> Result<Option<String>, RunnerError> {
    let ports = RunnerBuiltinPorts::new();
    try_run_builtin_task(
        &ports,
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )
    .map_err(RunnerError::from)
}

fn removed_builtin_invocation_error(selector: &TaskSelector) -> Option<RunnerError> {
    if !matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
        return None;
    }
    let request = selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone());
    Some(RunnerError::task_invocation(format!(
        "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
    )))
}

fn resolve_deferred_selection_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<Option<String>, RunnerError> {
    if !should_attempt_deferral(selection_error) {
        return Ok(None);
    }
    let Some(deferral) = select_deferral(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    ) else {
        return Ok(None);
    };
    run_deferred_request(
        task,
        &preflight.runtime_args_raw,
        &deferral,
        selection_error,
    )
    .map(Some)
}
