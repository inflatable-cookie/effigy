#[path = "pipeline/command.rs"]
mod command;
#[path = "pipeline/managed.rs"]
pub(super) mod managed;
#[path = "pipeline/standard.rs"]
pub(super) mod standard;

use effigy_cli::TaskInvocation;

use super::planning::ExecutionPreflight;
use super::render::render_task_plan;
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

    if preflight.plan {
        return render_resolved_selector_plan(&preflight, &selection);
    }

    if let Some(output) = managed::run_managed_task(&preflight, &selection, &selection_plan)? {
        return Ok(output);
    }

    standard::run_standard_task(&preflight, &selection, &selection_plan)
}

fn render_resolved_selector_plan(
    preflight: &ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<String, RunnerError> {
    let env_schema_catalog = effigy_manifest::env_schema_declaring_catalog(
        &preflight.catalogs,
        &selection.catalog.catalog_root,
    );
    let env_schema_resolved = standard::resolve_env_schema_if_present(
        env_schema_catalog.map_or(selection.catalog.catalog_root.as_path(), |catalog| {
            catalog.catalog_root.as_path()
        }),
        preflight.runtime_args_raw.env_schema_override.as_deref(),
        env_schema_catalog.and_then(|catalog| catalog.manifest.env_schema.as_ref()),
    )?;
    let command = command::build_task_command(preflight, selection, &env_schema_resolved)?;
    render_task_plan(
        preflight.output_json,
        &preflight.selector,
        preflight.task_execution_root(&selection.catalog.catalog_root),
        &command,
        selection,
    )
}
