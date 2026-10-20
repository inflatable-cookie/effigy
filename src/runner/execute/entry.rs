use std::collections::BTreeMap;

use effigy_cli::TaskInvocation;
use effigy_execution::{ExecutionDispatchPlan, ExecutionPreflightInput, TaskExecutionRequest};
use effigy_manifest::TaskSelection;

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight_from_input;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::RunKey;

fn run_manifest_task_with_preflight_input(
    task: &TaskInvocation,
    mut input: ExecutionPreflightInput,
) -> Result<String, RunnerError> {
    let (task, run_key) = task_without_run_key(task)?;
    if run_key.is_some() {
        input.args = task.args.clone();
    }
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        !preflight.plan && preflight.selector.task_name == "dev",
    );
    if preflight.plan {
        refuse_key_without_run(run_key.as_ref(), "`--plan` starts no run")?;
        return run_execution_pipeline(&task, preflight);
    }
    let (selection, selection_plan) =
        match super::selection::resolve_task_selection(&task, &preflight)? {
            super::selection::SelectionResolution::Selected { selection, plan } => {
                (selection, plan)
            }
            super::selection::SelectionResolution::Output(output) => {
                refuse_key_without_run(run_key.as_ref(), "this selector starts no task run")?;
                return Ok(output);
            }
        };
    run_selected_task(&preflight, &selection, &selection_plan, run_key.as_ref())
}

fn run_manifest_task_with_preflight_input_and_env(
    task: &TaskInvocation,
    mut input: ExecutionPreflightInput,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let (task, run_key) = task_without_run_key(task)?;
    if run_key.is_some() {
        input.args = task.args.clone();
    }
    let preflight = build_execution_preflight_from_input(input)?;
    let _local_dev_secrets = crate::runner::secret_session::activate_local_dev_secret_access(
        !preflight.plan && preflight.selector.task_name == "dev",
    );
    if preflight.plan {
        refuse_key_without_run(run_key.as_ref(), "`--plan` starts no run")?;
        return run_execution_pipeline(&task, preflight);
    }
    let (selection, selection_plan) =
        match super::selection::resolve_task_selection(&task, &preflight)? {
            super::selection::SelectionResolution::Selected { selection, plan } => {
                (selection, plan)
            }
            super::selection::SelectionResolution::Output(output) => {
                refuse_key_without_run(run_key.as_ref(), "this selector starts no task run")?;
                return Ok(output);
            }
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
    run_selected_task(
        &preflight,
        &overridden_selection,
        &selection_plan,
        run_key.as_ref(),
    )
}

/// Runs the selected task. A `--run-id` key that already names a run
/// attaches to that run and exits with its result: a repeat never starts a
/// second run (Q-006). Heavy admission happens inside the run's admission.
fn run_selected_task(
    preflight: &super::planning::ExecutionPreflight,
    selection: &TaskSelection<'_>,
    selection_plan: &effigy_execution::ExecutionSelectionPlan,
    run_key: Option<&RunKey>,
) -> Result<String, RunnerError> {
    let result =
        super::pipeline::managed::run_managed_task(preflight, selection, selection_plan, run_key)
            .and_then(|output| match output {
                Some(output) => Ok(output),
                None => super::pipeline::standard::run_standard_task(
                    preflight,
                    selection,
                    selection_plan,
                    run_key,
                ),
            });
    match result {
        Err(RunnerError::RunKeyExists { run }) => {
            crate::runner::run_control::attach(&run, preflight.output_json)
        }
        other => other,
    }
}

fn refuse_key_without_run(run_key: Option<&RunKey>, why: &str) -> Result<(), RunnerError> {
    match run_key {
        Some(key) => Err(RunnerError::task_invocation(format!(
            "`--run-id {}` names a task run, but {why}",
            key.key()
        ))),
        None => Ok(()),
    }
}

fn task_without_run_key(
    task: &TaskInvocation,
) -> Result<(TaskInvocation, Option<RunKey>), RunnerError> {
    let marker = effigy_cli::INTERNAL_RUN_ID_ARG;
    let Some(index) = task.args.iter().position(|arg| arg == marker) else {
        return Ok((task.clone(), None));
    };
    let key = match task.args.get(index + 1) {
        Some(key) if index + 2 == task.args.len() => RunKey::parse(key)?,
        _ => {
            return Err(RunnerError::task_invocation(
                "internal `--run-id` argument is malformed",
            ))
        }
    };
    let mut args = task.args.clone();
    args.truncate(index);
    Ok((
        TaskInvocation {
            name: task.name.clone(),
            args,
        },
        Some(key),
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
