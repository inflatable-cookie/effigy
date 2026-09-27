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
use effigy_managed::resolve_managed_task_plan;

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
    let command = plan_command_shape(preflight, selection)?;
    render_task_plan(
        preflight.output_json,
        &preflight.selector,
        preflight.task_execution_root(&selection.catalog.catalog_root),
        &command,
        selection,
    )
}

fn plan_command_shape(
    preflight: &ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<String, RunnerError> {
    if selection.task.mode.as_deref() == Some("tui") {
        let Some(plan) = resolve_managed_task_plan(
            &preflight.selector,
            selection.catalog,
            selection.task,
            &preflight.runtime_args_exec,
            &preflight.catalogs,
            &selection.catalog.catalog_root,
            &effigy_routing::resolve_task_selection,
        )?
        else {
            return Err(RunnerError::TaskMissingRunCommand {
                task: preflight.selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            });
        };
        return Ok(plan
            .processes
            .iter()
            .map(|process| format!("{}: {}", process.name, process.run))
            .collect::<Vec<_>>()
            .join("; "));
    }
    command::build_task_command(preflight, selection, &None)
}
