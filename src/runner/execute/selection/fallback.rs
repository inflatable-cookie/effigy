use effigy_cli::TaskInvocation;

use super::super::planning::ExecutionPreflight;
use super::super::selection::result;
use super::super::selection::SelectionResolution;
use crate::runner::builtin_ports::RunnerBuiltinPorts;
use crate::runner::command_context::active_runtime_context;
use crate::runner::deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use crate::runner::error::RunnerError;
use crate::runner::exec_command::try_run_exec_alias;
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

    let prefer_deferral = should_try_deferral_before_exec_alias();
    if prefer_deferral {
        if let Some(output) = resolve_deferred_selection_output(task, preflight, &error)? {
            return Ok(result::output(output));
        }
    }

    if let Some(output) = resolve_exec_alias_output(task, preflight, &error)? {
        return Ok(result::output(output));
    }
    if !prefer_deferral {
        if let Some(output) = resolve_deferred_selection_output(task, preflight, &error)? {
            return Ok(result::output(output));
        }
    }
    Err(error)
}

fn should_try_deferral_before_exec_alias() -> bool {
    active_runtime_context().is_some_and(|context| context.container().inside_container_handoff)
}

fn resolve_builtin_selection_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
) -> Result<Option<String>, RunnerError> {
    let ports = RunnerBuiltinPorts::new();
    // The registry built-in owns this invocation only now that manifest
    // selection failed. Record it so the direct CLI route can warn about the
    // displaced direct spelling on success, usage-error, and runtime-error
    // envelopes alike; grouped and nested executions never have a recording
    // scope open. `Ok(None)` means the name is not a registry built-in (or
    // its target root did not resolve), so nothing was selected.
    match try_run_builtin_task(
        &ports,
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(Some(output)) => {
            crate::cli::legacy_direct::record_registry_warning(&task.name);
            Ok(Some(output))
        }
        Ok(None) => Ok(None),
        Err(error) => {
            crate::cli::legacy_direct::record_registry_warning(&task.name);
            Err(error.into())
        }
    }
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

fn resolve_exec_alias_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<Option<String>, RunnerError> {
    if preflight.selector.prefix.is_some() {
        return Ok(None);
    }
    if !matches!(
        selection_error,
        RunnerError::TaskNotFound { .. } | RunnerError::TaskNotFoundAny { .. }
    ) {
        return Ok(None);
    }
    try_run_exec_alias(
        &preflight.resolved.resolved_root,
        &preflight.invocation_cwd,
        &task.name,
        &preflight.runtime_args_exec.passthrough,
        preflight.output_json,
    )
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
