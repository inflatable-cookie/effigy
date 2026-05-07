#[path = "pipeline/command.rs"]
mod command;
#[path = "pipeline/managed.rs"]
pub(super) mod managed;
#[path = "pipeline/standard.rs"]
pub(super) mod standard;

use effigy_cli::TaskInvocation;

use super::planning::ExecutionPreflight;
use super::selection::{resolve_task_selection, SelectionResolution};
use crate::runner::error::RunnerError;

pub(super) fn run_execution_pipeline(
    task: &TaskInvocation,
    preflight: ExecutionPreflight,
) -> Result<String, RunnerError> {
    let (selection, selection_plan) = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected { selection, plan } => (selection, plan),
        SelectionResolution::Output(output) => return Ok(output),
    };

    if let Some(output) = managed::run_managed_task(&preflight, &selection, &selection_plan)? {
        return Ok(output);
    }

    standard::run_standard_task(&preflight, &selection, &selection_plan)
}
