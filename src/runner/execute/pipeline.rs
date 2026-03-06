#[path = "pipeline/command.rs"]
mod command;
#[path = "pipeline/managed.rs"]
mod managed;
#[path = "pipeline/standard.rs"]
mod standard;

use crate::TaskInvocation;

use super::preflight::ExecutionPreflight;
use super::selection::{resolve_task_selection, SelectionResolution};
use crate::runner::error::RunnerError;

pub(super) fn run_execution_pipeline(
    task: &TaskInvocation,
    preflight: ExecutionPreflight,
) -> Result<String, RunnerError> {
    let selection = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected(selection) => selection,
        SelectionResolution::Output(output) => return Ok(output),
    };

    if let Some(output) = managed::run_managed_task(&preflight, &selection)? {
        return Ok(output);
    }

    standard::run_standard_task(&preflight, &selection)
}
