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
        Ok(selection) => {
            let plan = build_execution_selection_plan(preflight, &selection);
            Ok(result::selected(selection, plan))
        }
        Err(error) => resolve_selection_error(task, preflight, error.into()),
    }
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

fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    fallback::resolve_selection_error(task, preflight, error)
}
