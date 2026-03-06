use crate::runner::{ManifestManagedRunStep, RunnerError};

use super::super::super::scheduler;
use super::projection::ProjectedRunSequence;

pub(super) fn render_projected_run_sequence(
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
        None if projected.has_non_default_policy => Some(
            (0..projected.commands.len())
                .map(|index| vec![index])
                .collect::<Vec<Vec<usize>>>(),
        ),
        None => None,
    }
}
