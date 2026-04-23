#[path = "selection/fallback.rs"]
mod fallback;
#[path = "selection/result.rs"]
mod result;

use effigy_cli::TaskInvocation;

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;
use effigy_routing::select_catalog_and_task;

pub(super) use result::SelectionResolution;

pub(super) fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    match select_catalog_and_task(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(selection) => Ok(result::selected(selection)),
        Err(error) => resolve_selection_error(task, preflight, error.into()),
    }
}

fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    fallback::resolve_selection_error(task, preflight, error)
}
