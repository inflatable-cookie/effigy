use effigy_cli::TaskInvocation;

use super::super::planning::ExecutionPreflight;
use super::super::route::Route;
use super::super::selection::result;
use super::super::selection::SelectionResolution;
use crate::runner::builtin_ports::RunnerBuiltinPorts;
use crate::runner::deferral::run_deferred_request;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::try_run_exec_alias;
use effigy_builtin::try_run_builtin_task;

/// Carries out the route the one resolver chose for a selector that names
/// no declared task.
pub(super) fn run_fallback_route<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    route: Route<'a>,
) -> Result<SelectionResolution<'a>, RunnerError> {
    match route {
        Route::Task { selection, plan } => Ok(SelectionResolution::Selected { selection, plan }),
        Route::Builtin => {
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
            .map_err(RunnerError::from)?
            .map(result::output)
            .ok_or_else(|| route_vanished(preflight, "built-in"))
        }
        Route::ExecAlias => try_run_exec_alias(
            &preflight.resolved.resolved_root,
            &preflight.invocation_cwd,
            &task.name,
            &preflight.runtime_args_exec.passthrough,
            preflight.output_json,
        )?
        .map(result::output)
        .ok_or_else(|| route_vanished(preflight, "exec alias")),
        Route::Defer { deferral, cause } => {
            run_deferred_request(task, &preflight.runtime_args_raw, &deferral, &cause)
                .map(result::output)
        }
    }
}

fn route_vanished(preflight: &ExecutionPreflight, kind: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "{kind} `{}` disappeared between resolution and execution",
        preflight.selector.task_name
    ))
}
