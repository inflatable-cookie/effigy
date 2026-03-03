use std::collections::BTreeMap;
use std::path::Path;

use crate::runner::manifest::{ManifestEnvEntry, ManifestEnvFileDirective};
use crate::runner::{LoadedCatalog, ManifestManagedRunStep, RunnerError};

use super::super::scheduler;
use super::command::wrap_command_with_task_env;
use super::run_step::resolve_task_run_step;
use self::env_resolution::StepEnvAccumulator;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;

pub(super) fn render_run_sequence(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    task_env_file: Option<&ManifestEnvFileDirective>,
    env_profiles: &BTreeMap<String, ManifestEnvEntry>,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if steps.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` has an empty run array"
        )));
    }

    let mut commands = Vec::with_capacity(steps.len());
    let mut policies = Vec::with_capacity(steps.len());
    let mut env_state = StepEnvAccumulator::new(task_env_file)?;

    for step in steps {
        env_state.apply_from_step(task_name, step, env_profiles, repo_root, catalogs)?;
        let command = resolve_task_run_step(
            task_name,
            step,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth,
        )?;
        commands.push(wrap_command_with_task_env(
            command,
            env_state.chained_env(),
            repo_root,
        ));
        policies.push(scheduler::step_policy_for(step));
    }

    render_run_sequence_with_schedule(task_name, steps, &commands, &policies)
}

fn render_run_sequence_with_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    commands: &[String],
    policies: &[scheduler::RunStepPolicy],
) -> Result<String, RunnerError> {
    let has_non_default_policy = policies.iter().copied().any(|policy| !policy.is_default());
    let schedule = scheduler::build_run_sequence_schedule(task_name, steps)?;
    match schedule {
        Some(levels) => Ok(scheduler::render_parallel_run_levels_with_policy(
            commands, &levels, policies,
        )),
        None if has_non_default_policy => {
            let sequential_levels = (0..commands.len())
                .map(|index| vec![index])
                .collect::<Vec<Vec<usize>>>();
            Ok(scheduler::render_parallel_run_levels_with_policy(
                commands,
                &sequential_levels,
                policies,
            ))
        }
        None => Ok(commands.join(" && ")),
    }
}
