use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_execution::{ExecutionDispatchPlan, TaskExecutionRequest};

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight;
use crate::runner::error::RunnerError;

pub(in crate::runner) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    run_execution_pipeline(task, preflight)
}

pub(in crate::runner) fn run_manifest_task_with_cwd_and_env(
    task: &TaskInvocation,
    cwd: PathBuf,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    let selection = match super::selection::resolve_task_selection(task, &preflight)? {
        super::selection::SelectionResolution::Selected(selection) => selection,
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

    if let Some(output) =
        super::pipeline::managed::run_managed_task(&preflight, &overridden_selection)?
    {
        return Ok(output);
    }

    super::pipeline::standard::run_standard_task(&preflight, &overridden_selection)
}

pub(in crate::runner) fn run_manifest_task_request(
    request: TaskExecutionRequest,
) -> Result<String, RunnerError> {
    let plan = ExecutionDispatchPlan::from_request(request)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let invocation = TaskInvocation {
        name: plan.selector.clone(),
        args: plan.args.clone(),
    };

    if plan.request.environment.env.is_empty() {
        return run_manifest_task_with_cwd(&invocation, plan.effective_cwd);
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
    run_manifest_task_with_cwd_and_env(&invocation, plan.effective_cwd, &env_overrides)
}
