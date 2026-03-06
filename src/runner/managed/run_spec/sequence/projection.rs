use crate::runner::error::RunnerError;
use crate::runner::manifest::task_runtime::ManifestManagedRunStep;

use super::super::super::scheduler;
use super::super::command::wrap_command_with_task_env;
use super::super::run_step::resolve_task_run_step;
use super::super::RunSpecContext;
use super::env_resolution::StepEnvAccumulator;

pub(super) struct ProjectedRunSequence {
    pub(super) commands: Vec<String>,
    pub(super) policies: Vec<scheduler::RunStepPolicy>,
    pub(super) has_non_default_policy: bool,
}

pub(super) fn project_run_sequence(
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
