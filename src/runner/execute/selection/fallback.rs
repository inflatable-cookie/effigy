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
use effigy_builtin::{try_run_builtin_task, TestRunRequirement};

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
            let requirement = if builtin_runs_project_tests(&preflight.selector.task_name)
                && !preflight.plan
                && !preflight.help
            {
                effigy_builtin::test_run_requirement(
                    &preflight.selector,
                    &preflight.runtime_args_raw,
                    &preflight.resolved.resolved_root,
                    &preflight.catalogs,
                )?
            } else {
                TestRunRequirement::Observe
            };
            let label = format!("effigy {}", preflight.selector.task_name);
            let root = &preflight.resolved.resolved_root;
            let mut suite_run = match requirement {
                TestRunRequirement::Observe => None,
                TestRunRequirement::Checkout => Some(if crate::runner::scratch::wanted() {
                    TaskRun::standalone_scratching(root, &label)?
                } else {
                    TaskRun::standalone_custody(root, &label)?
                }),
                TestRunRequirement::Local(sources) => {
                    let local =
                        super::super::sequence_run::foreground_test_sources(preflight, sources)?;
                    Some(if crate::runner::scratch::wanted() {
                        TaskRun::standalone_scratching(root, &label)?
                    } else if !local || preflight.task_source.is_some() {
                        TaskRun::standalone_custody(root, &label)?
                    } else {
                        TaskRun::standalone_foreground(root, &label)?
                    })
                }
            };
            let outcome = match suite_run.as_mut() {
                Some(run) => {
                    let _scope = run.enter();
                    let ports = RunnerBuiltinPorts::with_suite_run(run);
                    try_run_builtin_task(
                        &ports,
                        &preflight.selector,
                        task,
                        &preflight.runtime_args_raw,
                        &preflight.resolved.resolved_root,
                        &preflight.catalogs,
                        &preflight.invocation_cwd,
                    )
                }
                None => try_run_builtin_task(
                    &RunnerBuiltinPorts::new(),
                    &preflight.selector,
                    task,
                    &preflight.runtime_args_raw,
                    &preflight.resolved.resolved_root,
                    &preflight.catalogs,
                    &preflight.invocation_cwd,
                ),
            };
            // The builtin's own outcome is carried into the runner's error
            // before the run closes: a cancellation the builtin reported must
            // not become a failed run.
            let outcome = outcome.map_err(RunnerError::from);
            if let Some(run) = suite_run {
                let workload = match &outcome {
                    Ok(Some(_)) => WorkloadOutcome::Succeeded,
                    Ok(None) => WorkloadOutcome::NotStarted,
                    Err(error) => error.workload_outcome(),
                };
                for warning in run.close(workload) {
                    eprintln!("warning: {}: {warning}", preflight.selector.task_name);
                }
            }
            match outcome? {
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
