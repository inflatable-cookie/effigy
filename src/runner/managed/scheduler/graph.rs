#[path = "graph/cycle.rs"]
mod cycle;
#[path = "graph/dependencies.rs"]
mod dependencies;
#[path = "graph/index.rs"]
mod index;
#[path = "graph/topological.rs"]
mod topological;

use super::super::super::{ManifestManagedRunStep, RunnerError};

use cycle::detect_dependency_cycle;
use dependencies::{build_step_dependencies, build_step_dependents};
use index::build_step_index;
use topological::build_schedule_levels;

pub(super) fn build_run_sequence_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<Option<Vec<Vec<usize>>>, RunnerError> {
    let step_index = build_step_index(task_name, steps)?;
    if !step_index.has_explicit_dependencies {
        return Ok(None);
    }

    let dependencies = build_step_dependencies(task_name, steps, &step_index.id_to_index)?;
    let dependents = build_step_dependents(&dependencies);

    if let Some(cycle) = detect_dependency_cycle(&dependencies, &step_index.display_names) {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` run sequence contains dependency cycle: {}",
            cycle.join(" -> ")
        )));
    }

    let Some(levels) = build_schedule_levels(steps.len(), &dependencies, &dependents) else {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` run sequence contains dependency cycle"
        )));
    };

    Ok(Some(levels))
}

#[cfg(test)]
#[path = "graph/tests.rs"]
mod tests;
