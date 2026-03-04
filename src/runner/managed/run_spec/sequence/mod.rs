use crate::runner::{ManifestManagedRunStep, RunnerError};

use self::env_resolution::StepEnvAccumulator;
use super::super::scheduler;
use super::command::wrap_command_with_task_env;
use super::run_step::resolve_task_run_step;
use super::RunSpecContext;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;

pub(super) fn render_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<String, RunnerError> {
    if steps.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{}` has an empty run array",
            context.task_name
        )));
    }

    let mut commands = Vec::with_capacity(steps.len());
    let mut policies = Vec::with_capacity(steps.len());
    let mut env_state = StepEnvAccumulator::new(context.task_env_file)?;

    for step in steps {
        env_state.apply_from_step(
            context.task_name,
            step,
            context.env_profiles,
            context.repo_root,
            context.catalogs,
        )?;
        let command = resolve_task_run_step(step, context)?;
        commands.push(wrap_command_with_task_env(
            command,
            env_state.chained_env(),
            context.repo_root,
        ));
        policies.push(scheduler::step_policy_for(step));
    }

    render_run_sequence_with_schedule(context.task_name, steps, &commands, &policies)
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
