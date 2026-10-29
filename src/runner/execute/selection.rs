#[path = "selection/fallback.rs"]
mod fallback;
#[path = "selection/result.rs"]
mod result;

use effigy_cli::TaskInvocation;
use effigy_execution::{
    ExecutionSelectionCatalogSummary, ExecutionSelectionInput, ExecutionSelectionPlan,
};

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;

pub(in crate::runner) use result::SelectionResolution;

/// Resolves `task` through the one resolver and carries out a non-task
/// route (built-in, exec alias, deferral) at once.
pub(in crate::runner) fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    let route = super::route::resolve_route(task, preflight)?;
    fallback::run_fallback_route(task, preflight, route)
}

pub(super) fn build_execution_selection_plan(
    preflight: &ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> ExecutionSelectionPlan {
    ExecutionSelectionPlan::new(
        ExecutionSelectionInput::from_discovery(&preflight.discovery_plan),
        ExecutionSelectionCatalogSummary {
            alias: selection.catalog.alias.clone(),
            catalog_root: selection.catalog.catalog_root.clone(),
            manifest_path: selection.catalog.manifest_path.clone(),
            depth: selection.catalog.depth,
        },
        selection.mode,
        selection.evidence.clone(),
        preflight.selector.task_name.clone(),
    )
}
