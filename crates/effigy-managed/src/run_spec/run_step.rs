use effigy_manifest::ManifestManagedRunStepTable;

use super::super::references;
use super::command::{render_rhai_step_invocation, render_step_command};
use super::RunSpecContext;
use crate::ManagedError;
use effigy_manifest::ManifestManagedRunStep;

pub fn resolve_task_run_step(
    step: &ManifestManagedRunStep,
    context: RunSpecContext<'_>,
) -> Result<String, ManagedError> {
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
) -> Result<String, ManagedError> {
    let selection = select_run_or_task(
        step.run.as_deref(),
        step.task.as_deref(),
        step.rhai.as_deref(),
        step.env.is_some() || step.env_file.is_some(),
        || {
            ManagedError::task_invocation(format!(
                "task `{}` run step is invalid: define exactly one of `run`, `task`, or `rhai`",
                context.task_name
            ))
        },
        || {
            ManagedError::task_invocation(format!(
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
) -> Result<String, ManagedError> {
    match selection {
        RunOrTaskRef::Run(run) => Ok(render_step_command(run, context)),
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_step(
            context.task_name,
            task_ref,
            references::ReferenceResolution {
                args_rendered: context.args_rendered,
                catalogs: context.catalogs,
                task_scope_cwd: context.task_scope_cwd,
                execution_root: context.repo_root,
                invocation_cwd: context.invocation_cwd,
                runtime_env_schema_override: context.runtime_env_schema_override,
                depth: context.depth,
                resolver: context.resolver,
                host_launched: true,
            },
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
) -> Result<RunOrTaskRef<'a>, ManagedError>
where
    FBoth: FnOnce() -> ManagedError,
    FNone: FnOnce() -> ManagedError,
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
