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

struct ProjectedRunSequence {
    commands: Vec<String>,
    policies: Vec<scheduler::RunStepPolicy>,
    has_non_default_policy: bool,
}

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

    let projected = project_run_sequence(steps, context)?;
    render_projected_run_sequence(context.task_name, steps, &projected)
}

fn project_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<ProjectedRunSequence, RunnerError> {
    let mut commands = Vec::<String>::with_capacity(steps.len());
    let mut policies = Vec::<scheduler::RunStepPolicy>::with_capacity(steps.len());
    let mut has_non_default_policy = false;
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
        let policy = scheduler::step_policy_for(step);
        has_non_default_policy |= !policy.is_default();
        policies.push(policy);
    }

    Ok(ProjectedRunSequence {
        commands,
        policies,
        has_non_default_policy,
    })
}

fn render_projected_run_sequence(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    projected: &ProjectedRunSequence,
) -> Result<String, RunnerError> {
    let schedule = scheduler::build_run_sequence_schedule(task_name, steps)?;
    let levels = render_schedule_levels(schedule, projected);
    if let Some(levels) = levels {
        return Ok(scheduler::render_parallel_run_levels_with_policy(
            &projected.commands,
            &levels,
            &projected.policies,
        ));
    }
    Ok(projected.commands.join(" && "))
}

fn render_schedule_levels(
    schedule: Option<Vec<Vec<usize>>>,
    projected: &ProjectedRunSequence,
) -> Option<Vec<Vec<usize>>> {
    match schedule {
        Some(levels) => Some(levels),
        None if projected.has_non_default_policy => {
            let sequential_levels = (0..projected.commands.len())
                .map(|index| vec![index])
                .collect::<Vec<Vec<usize>>>();
            Some(sequential_levels)
        }
        None => None,
    }
}
