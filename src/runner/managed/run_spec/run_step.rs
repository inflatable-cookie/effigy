use crate::runner::manifest::ManifestManagedRunStepTable;

use super::super::super::{ManifestManagedRunStep, RunnerError};
use super::super::references;
use super::command::render_command_template;
use super::RunSpecContext;

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
                references::resolve_task_reference_step(
                    context.task_name,
                    task_ref,
                    context.args_rendered,
                    context.catalogs,
                    context.task_scope_cwd,
                    context.depth,
                )
            } else {
                Ok(render_command_template(
                    command,
                    context.repo_root,
                    context.args_rendered,
                ))
            }
        }
        ManifestManagedRunStep::Step(step) => resolve_table_task_run_step(step, context),
    }
}

fn resolve_table_task_run_step(
    step: &ManifestManagedRunStepTable,
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    match select_run_or_task(
        step.run.as_deref(),
        step.task.as_deref(),
        step.env.is_some() || step.env_file.is_some(),
        || {
            RunnerError::task_invocation(format!(
                "task `{}` run step is invalid: define either `run` or `task`, not both",
                context.task_name
            ))
        },
        || {
            RunnerError::task_invocation(format!(
                "task `{}` run step is invalid: missing both `run` and `task`",
                context.task_name
            ))
        },
    )? {
        RunOrTaskRef::Run(run) => Ok(render_command_template(
            run,
            context.repo_root,
            context.args_rendered,
        )),
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_step(
            context.task_name,
            task_ref,
            context.args_rendered,
            context.catalogs,
            context.task_scope_cwd,
            context.depth,
        ),
        RunOrTaskRef::Noop => Ok(":".to_owned()),
    }
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
    Noop,
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
    has_env_directive: bool,
    both_error: FBoth,
    none_error: FNone,
) -> Result<RunOrTaskRef<'a>, RunnerError>
where
    FBoth: FnOnce() -> RunnerError,
    FNone: FnOnce() -> RunnerError,
{
    match (run, task) {
        (Some(run), None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task)) => Ok(RunOrTaskRef::Task(task)),
        (Some(_), Some(_)) => Err(both_error()),
        (None, None) if has_env_directive => Ok(RunOrTaskRef::Noop),
        (None, None) => Err(none_error()),
    }
}
