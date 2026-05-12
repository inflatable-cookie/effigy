use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_context::EffigyRuntimeContext;
use effigy_execution::{
    ExecutionEnvironmentPlan, ExecutionSurface, TaskExecutionRequest, TaskExecutionRequestBuilder,
};
use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn, TaskSelection};
use effigy_tasks::CatalogSelectionMode;

use super::planning::{
    build_execution_preflight as build_execution_preflight_impl, ExecutionPreflight,
};
use super::selection::{resolve_task_selection, SelectionResolution};
use crate::runner::error::RunnerError;

pub(in crate::runner) use super::binding::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution,
    ContainerExecutionBinding, ExecutionBindingKind, ExecutionBindingResolution,
    InlineWorkspaceCapabilitySurface,
};

pub(in crate::runner) fn run_manifest_task_request(
    request: TaskExecutionRequest,
) -> Result<String, RunnerError> {
    super::entry::run_manifest_task_request(request)
}

#[cfg(test)]
pub(in crate::runner) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface(task, cwd, ExecutionSurface::DirectCli)
}

pub(in crate::runner) fn run_manifest_task_with_surface(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface_and_env(task, cwd, surface, &BTreeMap::new())
}

pub(in crate::runner) fn run_manifest_task_with_surface_and_env(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    run_manifest_task_with_surface_env_and_secret_targets(task, cwd, surface, env_overrides, &[])
}

pub(in crate::runner) fn run_manifest_task_with_surface_env_and_secret_targets(
    task: &TaskInvocation,
    cwd: PathBuf,
    surface: ExecutionSurface,
    env_overrides: &BTreeMap<String, String>,
    secret_targets: &[&str],
) -> Result<String, RunnerError> {
    let runtime_context = EffigyRuntimeContext::capture_lossy(Some(cwd.clone()), None)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let mut environment = ExecutionEnvironmentPlan::default().cwd(cwd);
    for (key, value) in env_overrides {
        environment = environment.env(key.clone(), value.clone());
    }
    for target in secret_targets {
        environment = environment.secret_target((*target).to_owned());
    }
    let request = TaskExecutionRequestBuilder::new()
        .runtime_context(runtime_context)
        .task(task.name.clone(), task.args.clone())
        .surface(surface)
        .environment(environment)
        .build()
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    run_manifest_task_request(request)
}

pub(in crate::runner) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    build_execution_preflight_impl(task, cwd)
}

pub(in crate::runner) fn task_requires_container_runtime(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<bool, RunnerError> {
    let preflight = build_execution_preflight_impl(task, cwd)?;
    let selection = match resolve_task_selection(task, &preflight)? {
        SelectionResolution::Selected { selection, .. } => selection,
        SelectionResolution::Output(_) => return Ok(false),
    };
    let binding_resolution = resolve_execution_binding_resolution(
        selection
            .catalog
            .manifest
            .task_defaults
            .as_ref()
            .and_then(|defaults| defaults.run_in),
        selection.catalog.manifest.systems.as_ref(),
        selection.catalog.manifest.containers.as_ref(),
        &preflight.selector.task_name,
        selection.task,
        "bootstrap backend selection",
    )?;
    Ok(binding_resolution.is_inline_container()
        || matches!(
            binding_resolution.kind(),
            ExecutionBindingKind::NamedContainer
        ))
}

pub(in crate::runner) fn run_managed_run_with_cwd(
    run: &ManifestManagedRun,
    cwd: PathBuf,
    label: &str,
) -> Result<String, RunnerError> {
    let invocation = TaskInvocation {
        name: label.to_owned(),
        args: Vec::new(),
    };
    let preflight = super::planning::build_execution_preflight(&invocation, cwd)?;
    let root_catalog = preflight
        .catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == preflight.resolved.resolved_root)
        .min_by_key(|catalog| catalog.depth)
        .ok_or_else(|| {
            RunnerError::task_invocation(format!(
                "bootstrap run could not resolve root catalog for {}",
                preflight.resolved.resolved_root.display()
            ))
        })?;

    let synthetic_task = ManifestTask {
        run: Some(run.clone()),
        run_in: Some(ManifestTaskRunIn::Host),
        ..Default::default()
    };
    let selection = TaskSelection {
        catalog: root_catalog,
        task: &synthetic_task,
        mode: CatalogSelectionMode::RootShallowest,
        evidence: vec!["bootstrap-local run".to_owned()],
    };
    let selection_plan = super::selection::build_execution_selection_plan(&preflight, &selection);
    super::pipeline::standard::run_standard_task(&preflight, &selection, &selection_plan)
}
