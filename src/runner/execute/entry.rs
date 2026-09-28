use std::collections::BTreeMap;

use effigy_cli::TaskInvocation;
use effigy_execution::{ExecutionDispatchPlan, ExecutionPreflightInput, TaskExecutionRequest};
use effigy_manifest::{ManifestTaskAdmission, TaskSelection};

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight_from_input;
use crate::runner::error::RunnerError;

fn run_manifest_task_with_preflight_input(
    task: &TaskInvocation,
    input: ExecutionPreflightInput,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        !preflight.plan && preflight.selector.task_name == "dev",
    );
    if preflight.plan {
        return run_execution_pipeline(task, preflight);
    }
    let (selection, selection_plan) =
        match super::selection::resolve_task_selection(task, &preflight)? {
            super::selection::SelectionResolution::Selected { selection, plan } => {
                (selection, plan)
            }
            super::selection::SelectionResolution::Output(output) => return Ok(output),
        };
    run_selected_task_with_admission(&preflight, &selection, || {
        if let Some(output) =
            super::pipeline::managed::run_managed_task(&preflight, &selection, &selection_plan)?
        {
            return Ok(output);
        }
        super::pipeline::standard::run_standard_task(&preflight, &selection, &selection_plan)
    })
}

fn run_manifest_task_with_preflight_input_and_env(
    task: &TaskInvocation,
    input: ExecutionPreflightInput,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        !preflight.plan && preflight.selector.task_name == "dev",
    );
    if preflight.plan {
        return run_execution_pipeline(task, preflight);
    }
    let (selection, selection_plan) =
        match super::selection::resolve_task_selection(task, &preflight)? {
            super::selection::SelectionResolution::Selected { selection, plan } => {
                (selection, plan)
            }
            super::selection::SelectionResolution::Output(output) => return Ok(output),
        };

    let mut overridden_task = selection.task.clone();
    for (key, value) in env_overrides {
        overridden_task.env.insert(key.clone(), value.clone());
    }
    let overridden_selection = effigy_manifest::TaskSelection {
        catalog: selection.catalog,
        task: &overridden_task,
        mode: selection.mode,
        evidence: selection.evidence,
        surface: selection.surface,
    };

    run_selected_task_with_admission(&preflight, &overridden_selection, || {
        if let Some(output) = super::pipeline::managed::run_managed_task(
            &preflight,
            &overridden_selection,
            &selection_plan,
        )? {
            return Ok(output);
        }
        super::pipeline::standard::run_standard_task(
            &preflight,
            &overridden_selection,
            &selection_plan,
        )
    })
}

fn run_selected_task_with_admission(
    preflight: &super::planning::ExecutionPreflight,
    selection: &TaskSelection<'_>,
    execute: impl FnOnce() -> Result<String, RunnerError>,
) -> Result<String, RunnerError> {
    if crate::runner::admission::scoped_lease_id().is_some() {
        return execute();
    }
    let task_name = preflight.selector.task_name.as_str();
    if is_managed_control_invocation(selection.task.mode.as_deref(), &preflight.runtime_args_exec)?
    {
        return execute();
    }
    let selected_heavy = matches!(selection.task.admission, Some(ManifestTaskAdmission::Heavy));
    if !selected_heavy && !matches!(task_name, "qa" | "ci" | "ci:fresh") {
        return execute();
    }

    let caller = std::env::var("EFFIGY_CALLER")
        .unwrap_or_else(|_| crate::runner::admission::default_caller_identity());
    let selector = preflight.selector.prefix.as_ref().map_or_else(
        || task_name.to_owned(),
        |prefix| format!("{prefix}/{task_name}"),
    );
    let lease = crate::runner::admission::acquire(crate::runner::admission::Request {
        caller: &caller,
        repository: &preflight.invocation_cwd,
        selector: &selector,
    })
    .map_err(RunnerError::task_invocation)?;
    let scope = crate::runner::admission::LeaseScope::enter(lease.id()).map_err(|error| {
        RunnerError::task_invocation(format!(
            "cannot install heavy-run signal forwarding: {error}"
        ))
    })?;
    let result = execute();
    lease.finish(&result);
    drop(scope);
    result
}

fn is_managed_control_invocation(
    mode: Option<&str>,
    runtime_args: &effigy_tasks::TaskRuntimeArgs,
) -> Result<bool, RunnerError> {
    if mode != Some("tui") {
        return Ok(false);
    }
    let invocation = effigy_managed::parse_managed_invocation(runtime_args)?;
    Ok(!matches!(
        invocation.action,
        effigy_managed::ManagedInvocation::Run { .. }
    ))
}

pub(in crate::runner) fn run_manifest_task_request(
    request: TaskExecutionRequest,
) -> Result<String, RunnerError> {
    let runtime_context = request.runtime_context.clone();
    if runtime_context.task_source().is_some() {
        crate::runner::command_context::with_runtime_context(&runtime_context, || {
            run_manifest_task_request_inner(request)
        })
    } else {
        run_manifest_task_request_inner(request)
    }
}

fn run_manifest_task_request_inner(request: TaskExecutionRequest) -> Result<String, RunnerError> {
    let plan = ExecutionDispatchPlan::from_request(request)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = TaskInvocation {
        name: plan.selector.clone(),
        args: plan.args.clone(),
    };
    let preflight_input = plan.preflight_input();

    if plan.request.environment.env.is_empty() {
        return run_manifest_task_with_preflight_input(&invocation, preflight_input);
    }

    let mut env_overrides = BTreeMap::new();
    for (key, value) in plan.request.environment.env {
        let value = value.into_string().map_err(|_| {
            RunnerError::task_invocation(format!(
                "execution request env override `{key}` is not valid UTF-8"
            ))
        })?;
        env_overrides.insert(key, value);
    }
    run_manifest_task_with_preflight_input_and_env(&invocation, preflight_input, &env_overrides)
}

#[cfg(test)]
mod tests {
    use super::is_managed_control_invocation;
    use effigy_tasks::TaskRuntimeArgs;

    fn args(passthrough: &[&str]) -> TaskRuntimeArgs {
        TaskRuntimeArgs {
            passthrough: passthrough
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
            ..TaskRuntimeArgs::default()
        }
    }

    #[test]
    fn managed_tui_controls_do_not_request_heavy_admission() {
        for action in ["status", "logs", "stop"] {
            assert!(
                is_managed_control_invocation(Some("tui"), &args(&[action])).expect("parse"),
                "{action} should bypass validation admission"
            );
        }
    }

    #[test]
    fn managed_tui_run_and_non_tui_tasks_are_not_controls() {
        assert!(!is_managed_control_invocation(Some("tui"), &args(&[])).expect("parse"));
        assert!(!is_managed_control_invocation(None, &args(&["status"]))
            .expect("non-managed task args are ignored"));
    }
}
