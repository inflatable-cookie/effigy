#[path = "selection/fallback.rs"]
mod fallback;
#[path = "selection/result.rs"]
mod result;

use effigy_cli::TaskInvocation;
use effigy_execution::{
    ExecutionSelectionCatalogSummary, ExecutionSelectionInput, ExecutionSelectionPlan,
    ExecutionSurface,
};

use super::planning::ExecutionPreflight;
use crate::runner::error::RunnerError;
use effigy_routing::select_catalog_and_task_on_surface;
use effigy_tasks::TaskSurface;

pub(super) use result::SelectionResolution;

pub(super) fn resolve_task_selection<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
) -> Result<SelectionResolution<'a>, RunnerError> {
    let surface = task_surface_for_execution(&preflight.execution_surface);
    match select_catalog_and_task_on_surface(
        surface,
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
    ) {
        Ok(selection) => {
            let plan = build_execution_selection_plan(preflight, &selection);
            Ok(result::selected(selection, plan))
        }
        // Draft selection never falls through to builtins, deferral, or exec
        // aliases: `effigy draft` selects only drafts and must fail closed.
        Err(error) if surface == TaskSurface::Draft => Err(error.into()),
        Err(error) => resolve_selection_error(task, preflight, error.into()),
    }
}

pub(super) fn task_surface_for_execution(surface: &ExecutionSurface) -> TaskSurface {
    match surface {
        ExecutionSurface::Draft => TaskSurface::Draft,
        _ => TaskSurface::Published,
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
