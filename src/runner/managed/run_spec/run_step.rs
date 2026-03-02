use std::path::Path;

use crate::runner::manifest::ManifestManagedRunStepTable;

use super::super::super::{LoadedCatalog, ManifestManagedRunStep, RunnerError};
use super::super::references;
use super::command::render_command_template;

pub(super) fn resolve_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStep,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                references::resolve_task_reference_step(
                    task_name,
                    task_ref,
                    args_rendered,
                    catalogs,
                    task_scope_cwd,
                    depth,
                )
            } else {
                Ok(render_command_template(command, repo_root, args_rendered))
            }
        }
        ManifestManagedRunStep::Step(step) => resolve_table_task_run_step(
            task_name,
            step,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth,
        ),
    }
}

fn resolve_table_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStepTable,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match select_run_or_task(
        step.run.as_deref(),
        step.task.as_deref(),
        || {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: define either `run` or `task`, not both"
            ))
        },
        || {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: missing both `run` and `task`"
            ))
        },
    )? {
        RunOrTaskRef::Run(run) => Ok(render_command_template(run, repo_root, args_rendered)),
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_step(
            task_name,
            task_ref,
            args_rendered,
            catalogs,
            task_scope_cwd,
            depth,
        ),
    }
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
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
        (None, None) => Err(none_error()),
    }
}
