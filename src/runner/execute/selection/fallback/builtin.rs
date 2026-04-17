use crate::TaskInvocation;

use super::super::super::super::builtin::try_run_builtin_task;
use super::super::super::preflight::ExecutionPreflight;
use crate::runner::error::RunnerError;
use effigy_tasks::TaskSelector;

pub(super) fn resolve_builtin_selection_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
) -> Result<Option<String>, RunnerError> {
    try_run_builtin_task(
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    )
}

pub(super) fn removed_builtin_invocation_error(selector: &TaskSelector) -> Option<RunnerError> {
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
