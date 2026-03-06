use crate::runner::error::RunnerError;
use crate::runner::manifest::task_runtime::ManifestManagedRunStep;

use super::RunSpecContext;
use projection::project_run_sequence;
use rendering::render_projected_run_sequence;

mod dotenv;
mod env_files;
mod env_resolution;
mod pathing;
mod projection;
mod rendering;

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
