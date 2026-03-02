use std::path::Path;

use crate::runner::manifest::ManifestManagedRunStepTable;

use super::super::util::shell_quote;
use super::super::{LoadedCatalog, ManifestManagedRun, ManifestManagedRunStep, RunnerError};
use super::references;
use super::scheduler;

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if depth > 12 {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run expansion exceeded maximum nested task references (12)"
        )));
    }
    match run {
        ManifestManagedRun::Command(command) => {
            Ok(render_command_template(command, repo_root, args_rendered))
        }
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "task `{task_name}` has an empty run array"
                )));
            }
            let mut commands = Vec::with_capacity(steps.len());
            let mut policies = Vec::with_capacity(steps.len());
            for step in steps {
                commands.push(resolve_task_run_step(
                    task_name,
                    step,
                    args_rendered,
                    repo_root,
                    catalogs,
                    task_scope_cwd,
                    depth + 1,
                )?);
                policies.push(scheduler::step_policy_for(step));
            }
            let has_non_default_policy =
                policies.iter().copied().any(|policy| !policy.is_default());
            let schedule = scheduler::build_run_sequence_schedule(task_name, steps)?;
            match schedule {
                Some(levels) => Ok(scheduler::render_parallel_run_levels_with_policy(
                    &commands, &levels, &policies,
                )),
                None if has_non_default_policy => {
                    let sequential_levels = (0..commands.len())
                        .map(|index| vec![index])
                        .collect::<Vec<Vec<usize>>>();
                    Ok(scheduler::render_parallel_run_levels_with_policy(
                        &commands,
                        &sequential_levels,
                        &policies,
                    ))
                }
                None => Ok(commands.join(" && ")),
            }
        }
    }
}

fn resolve_task_run_step(
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

fn render_command_template(command: &str, repo_root: &Path, args_rendered: &str) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    command
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered)
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
