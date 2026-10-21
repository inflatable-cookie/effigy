use effigy_cli::TaskInvocation;
use effigy_kernel::outcome::WorkloadOutcome;

use super::super::planning::ExecutionPreflight;
use super::super::route::{resolve_route_past_builtin, Route};
use super::super::selection::result;
use super::super::selection::SelectionResolution;
use crate::runner::builtin_ports::RunnerBuiltinPorts;
use crate::runner::deferral::run_deferred_request;
use crate::runner::error::RunnerError;
use crate::runner::exec_command::try_run_exec_alias;
use crate::runner::task_engine::TaskRun;
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
            // The built-in `test` route runs a project's suites on the host
            // exactly as a manifest test task would, so it holds a run of its
            // own: that run's scratch directory is the suites' temp directory,
            // and its settle decides retention.
            let scratch_run = (builtin_runs_project_tests(&preflight.selector.task_name)
                // `--plan` reads; it starts no suite and needs no directory.
                && !preflight.plan)
                .then(|| {
                    TaskRun::standalone_scratching(
                        &preflight.resolved.resolved_root,
                        &format!("effigy {}", preflight.selector.task_name),
                    )
                })
                .transpose()?;
            let ports = RunnerBuiltinPorts::new();
            let outcome = try_run_builtin_task(
                &ports,
                &preflight.selector,
                task,
                &preflight.runtime_args_raw,
                &preflight.resolved.resolved_root,
                &preflight.catalogs,
                &preflight.invocation_cwd,
            );
            if let Some(run) = scratch_run {
                let workload = match &outcome {
                    Ok(Some(_)) => WorkloadOutcome::Succeeded,
                    Ok(None) => WorkloadOutcome::NotStarted,
                    Err(_) => WorkloadOutcome::Failed { code: 1 },
                };
                for warning in run.close(workload) {
                    eprintln!("warning: {}: {warning}", preflight.selector.task_name);
                }
            }
            match outcome.map_err(RunnerError::from)? {
                Some(output) => Ok(result::output(output)),
                // The built-in declined at run time; the same order continues.
                None => run_fallback_route(
                    task,
                    preflight,
                    resolve_route_past_builtin(task, preflight)?,
                ),
            }
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

/// Whether this built-in selector runs project test commands on the host, and
/// therefore holds the same per-run scratch a manifest test task would.
fn builtin_runs_project_tests(task_name: &str) -> bool {
    task_name == "test"
}

fn route_vanished(preflight: &ExecutionPreflight, kind: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "{kind} `{}` disappeared between resolution and execution",
        preflight.selector.task_name
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn only_the_builtin_test_route_holds_a_project_test_run() {
        assert!(super::builtin_runs_project_tests("test"));
        for other in ["tasks", "config", "exec", "docs", "run"] {
            assert!(
                !super::builtin_runs_project_tests(other),
                "`{other}` runs no project test suites"
            );
        }
    }
}
