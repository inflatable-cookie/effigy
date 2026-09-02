use std::collections::BTreeMap;

use effigy_cli::TaskInvocation;
use effigy_execution::{
    ExecutionDispatchPlan, ExecutionPreflightInput, ExecutionSurface, TaskExecutionRequest,
};

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight_from_input;
use crate::runner::builtin_ports::RunnerBuiltinPorts;
use crate::runner::error::RunnerError;

fn run_manifest_task_with_preflight_input(
    task: &TaskInvocation,
    input: ExecutionPreflightInput,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        preflight.selector.task_name == "dev",
    );
    run_execution_pipeline(task, preflight)
}

fn run_manifest_task_with_preflight_input_and_env(
    task: &TaskInvocation,
    input: ExecutionPreflightInput,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        preflight.selector.task_name == "dev",
    );
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
    };

    if let Some(output) = super::pipeline::managed::run_managed_task(
        &preflight,
        &overridden_selection,
        &selection_plan,
    )? {
        return Ok(output);
    }

    super::pipeline::standard::run_standard_task(&preflight, &overridden_selection, &selection_plan)
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

/// Execute a grouped route that targets the built-in registry directly.
///
/// The grouped child route (`effigy <namespace> <child>` for a child such as
/// `config` or `scan` whose parse lives inside the built-in layer) is the
/// explicit built-in escape: manifest selector resolution is skipped
/// entirely, so a repository task or `[defer]` entry can never shadow it.
fn run_grouped_builtin_with_preflight_input(
    task: &TaskInvocation,
    input: ExecutionPreflightInput,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight_from_input(input)?;
    let ports = RunnerBuiltinPorts::new();
    match effigy_builtin::try_run_builtin_task(
        &ports,
        &preflight.selector,
        task,
        &preflight.runtime_args_raw,
        &preflight.resolved.resolved_root,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(Some(output)) => Ok(output),
        Ok(None) => Err(RunnerError::task_invocation(format!(
            "grouped route `{}` does not name a built-in command",
            task.name
        ))),
        Err(error) => Err(error.into()),
    }
}

fn run_manifest_task_request_inner(request: TaskExecutionRequest) -> Result<String, RunnerError> {
    let _depth_guard = crate::cli::legacy_direct::enter_execution_depth();
    let plan = ExecutionDispatchPlan::from_request(request)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = TaskInvocation {
        name: plan.selector.clone(),
        args: plan.args.clone(),
    };
    let preflight_input = plan.preflight_input();

    if plan.request.surface == ExecutionSurface::GroupedBuiltin {
        return run_grouped_builtin_with_preflight_input(&invocation, preflight_input);
    }

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
