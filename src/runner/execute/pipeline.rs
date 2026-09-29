#[path = "pipeline/command.rs"]
mod command;
#[path = "pipeline/managed.rs"]
pub(super) mod managed;
#[path = "pipeline/standard.rs"]
pub(super) mod standard;

use effigy_cli::TaskInvocation;
use effigy_manifest::ManifestTaskRunIn;

use super::planning::ExecutionPreflight;
use super::render::render_task_plan;
use super::selection::{resolve_task_selection, SelectionResolution};
use super::api::{
    effective_runtime_inputs, ensure_inline_workspace_supported,
    resolve_execution_binding_resolution, ExecutionBindingKind, InlineWorkspaceCapabilitySurface,
};
use super::json_payload::PlannedRuntimeTarget;
use super::routing::planned_container_target;
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
    let runtime = plan_runtime_target(preflight, selection)?;
    render_task_plan(
        preflight.output_json,
        &preflight.selector,
        preflight.task_execution_root(&selection.catalog.catalog_root),
        &command,
        selection,
        runtime.as_ref(),
    )
}

/// Resolve the runtime target a selector plan exposes.
///
/// This is the same authoritative binding resolution the execution path uses,
/// so a plan cannot promise host execution for a task whose runtime target is
/// a container — or silently plan host execution when the declared target is
/// missing or ambiguous.
fn plan_runtime_target(
    preflight: &ExecutionPreflight,
    selection: &effigy_manifest::TaskSelection<'_>,
) -> Result<Option<PlannedRuntimeTarget>, RunnerError> {
    if selection.task.mode.as_deref() == Some("tui") {
        // Managed runtimes are planned by the managed pipeline.
        return Ok(None);
    }
    let inputs =
        effective_runtime_inputs(&preflight.invocation_cwd, &preflight.catalogs, selection);
    let binding = resolve_execution_binding_resolution(
        inputs.default_run_in.clone(),
        inputs.systems.as_ref(),
        inputs.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "selector plan",
    )?;
    ensure_inline_workspace_supported(
        binding.binding(),
        InlineWorkspaceCapabilitySurface::StandardTaskRouting {
            task_name: &preflight.selector.task_name,
        },
    )?;
    match binding.kind() {
        ExecutionBindingKind::None
            if selection.task.effective_run_in(inputs.default_run_in)
                == ManifestTaskRunIn::Container =>
        {
            Err(RunnerError::task_invocation(format!(
                "task `{}` declares `run_in = \"container\"`, but no container target is defined",
                preflight.selector.task_name
            )))
        }
        ExecutionBindingKind::None | ExecutionBindingKind::Host => {
            Ok(Some(PlannedRuntimeTarget::Host))
        }
        ExecutionBindingKind::InlineContainer => Ok(Some(PlannedRuntimeTarget::InlineContainer)),
        ExecutionBindingKind::NamedContainer => {
            let (container, service) = planned_container_target(
                inputs.containers.as_ref(),
                binding.requested_container_name(),
            )?
            .ok_or_else(|| {
                RunnerError::task_invocation(format!(
                    "task `{}` resolves a container runtime target, but no container target is defined",
                    preflight.selector.task_name
                ))
            })?;
            Ok(Some(PlannedRuntimeTarget::Container {
                container,
                service,
                root: inputs.scope_root,
            }))
        }
    }
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
