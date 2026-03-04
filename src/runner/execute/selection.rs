use crate::TaskInvocation;

use super::super::catalog::select_catalog_and_task;
use super::super::deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use super::super::{try_run_builtin_task, RunnerError, TaskSelection, TaskSelector};
use super::preflight::ExecutionPreflight;

pub(super) enum SelectionResolution<'a> {
    Selected(TaskSelection<'a>),
    Output(String),
}

pub(super) fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    match select_catalog_and_task(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(selection) => Ok(SelectionResolution::Selected(selection)),
        Err(error) => resolve_selection_error(task, preflight, error),
    }
}

fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    if let Some(removed_builtin_error) = removed_builtin_invocation_error(&preflight.selector) {
        return Err(removed_builtin_error);
    }
    if let Some(output) = resolve_builtin_or_deferred_output(task, preflight, &error)? {
        return Ok(SelectionResolution::Output(output));
    }
    Err(error)
}

fn removed_builtin_invocation_error(selector: &TaskSelector) -> Option<RunnerError> {
    if !matches!(selector.task_name.as_str(), "repo-pulse" | "health") {
        return None;
    }
    let request = removed_builtin_request(selector);
    Some(RunnerError::task_invocation(format!(
        "`{request}` is no longer a built-in command. Use `effigy doctor` for consolidated health checks, or define `tasks.health` in your manifest for project-owned checks."
    )))
}

fn removed_builtin_request(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

fn resolve_builtin_or_deferred_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<Option<String>, RunnerError> {
    if let Some(output) = try_run_builtin_task(
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )? {
        return Ok(Some(output));
    }
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
