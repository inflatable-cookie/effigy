use crate::ManagedError;
use effigy_manifest::ManifestManagedRunStep;

use super::super::command::wrap_command_with_task_env;
use super::super::run_step::resolve_task_run_step;
use super::super::RunSpecContext;
use super::env_resolution::StepEnvAccumulator;

pub struct ProjectedRunSequence {
    pub commands: Vec<String>,
    pub timeouts: Vec<Option<u64>>,
}

pub fn project_run_sequence(
    steps: &[ManifestManagedRunStep],
    context: RunSpecContext<'_>,
) -> Result<ProjectedRunSequence, ManagedError> {
    let mut commands = Vec::<String>::with_capacity(steps.len());
    let mut timeouts = Vec::with_capacity(steps.len());
    let mut env_state =
        StepEnvAccumulator::new(context.task_env_file, context.runtime_env_schema_override)?;

    for step in steps {
        env_state.apply_from_step(
            context.task_name,
            step,
            context.env_profiles,
            context.repo_root,
            context.catalogs,
        )?;
        let command = resolve_task_run_step(step, context)?;
        // A host run in a runtime-scoped checkout resolves the canonical
        // declared backing-service alias in the value each step receives
        // (effigy#431, contract 005).
        let mut step_env = env_state.chained_env().clone();
        effigy_core::host_alias::resolve_env(&mut step_env, context.host_env_aliases);
        commands.push(wrap_command_with_task_env(
            command,
            &step_env,
            context.repo_root,
        ));
        timeouts.push(match step {
            ManifestManagedRunStep::Command(_) => None,
            ManifestManagedRunStep::Step(table) => table.timeout_ms,
        });
    }

    Ok(ProjectedRunSequence { commands, timeouts })
}
