use crate::runner::manifest::task_runtime::ManifestManagedRunStepTable;

use super::super::super::manifest::task_runtime::ManifestManagedRunStep;
use super::super::references;
use super::command::{render_rhai_step_invocation, render_step_command};
use super::RunSpecContext;
use crate::runner::error::RunnerError;

pub(super) fn resolve_task_run_step(
    step: &ManifestManagedRunStep,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                resolve_selected_run_or_task(RunOrTaskRef::Task(task_ref), context)
            } else {
                resolve_selected_run_or_task(RunOrTaskRef::Run(command), context)
            }
        }
        ManifestManagedRunStep::Step(step) => resolve_table_task_run_step(step.as_ref(), context),
    }
}

fn resolve_table_task_run_step(
    step: &ManifestManagedRunStepTable,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    let selection = select_run_or_task(
        step.run.as_deref(),
        step.task.as_deref(),
        step.rhai.as_deref(),
        step.env.is_some() || step.env_file.is_some(),
        || {
            RunnerError::task_invocation(format!(
                "task `{}` run step is invalid: define exactly one of `run`, `task`, or `rhai`",
                context.task_name
            ))
        },
        || {
            RunnerError::task_invocation(format!(
                "task `{}` run step is invalid: missing `run`, `task`, or `rhai`",
                context.task_name
            ))
        },
    )?;
    resolve_selected_run_or_task(selection, context)
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
    RhaiFile(&'a str),
    Noop,
}

fn resolve_selected_run_or_task(
    selection: RunOrTaskRef<'_>,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    match selection {
        RunOrTaskRef::Run(run) => Ok(render_step_command(run, context)),
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_step(
            context.task_name,
            task_ref,
            context.args_rendered,
            context.catalogs,
            context.task_scope_cwd,
            context.runtime_env_schema_override,
            context.depth,
            context.resolver,
        ),
        RunOrTaskRef::RhaiFile(path) => render_rhai_step_invocation(context, path),
        RunOrTaskRef::Noop => Ok(":".to_owned()),
    }
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
    rhai: Option<&'a str>,
    has_env_directive: bool,
    both_error: FBoth,
    none_error: FNone,
) -> Result<RunOrTaskRef<'a>, RunnerError>
where
    FBoth: FnOnce() -> RunnerError,
    FNone: FnOnce() -> RunnerError,
{
    let selected = [run.is_some(), task.is_some(), rhai.is_some()]
        .into_iter()
        .filter(|selected| *selected)
        .count();
    if selected > 1 {
        return Err(both_error());
    }

    match (run, task, rhai) {
        (Some(run), None, None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task), None) => Ok(RunOrTaskRef::Task(task)),
        (None, None, Some(path)) => Ok(RunOrTaskRef::RhaiFile(path)),
        (None, None, None) if has_env_directive => Ok(RunOrTaskRef::Noop),
        (None, None, None) => Err(none_error()),
        _ => Err(both_error()),
    }
}
