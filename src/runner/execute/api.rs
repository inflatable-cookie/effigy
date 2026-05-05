use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_manifest::{ManifestManagedRun, ManifestTask, ManifestTaskRunIn, TaskSelection};
use effigy_tasks::CatalogSelectionMode;

use super::binding::resolve_container_execution_binding as resolve_container_execution_binding_impl;
use super::entry::{
    run_manifest_task_with_cwd as run_manifest_task_with_cwd_impl,
    run_manifest_task_with_cwd_and_env as run_manifest_task_with_cwd_and_env_impl,
};
use super::planning::{
    build_execution_preflight as build_execution_preflight_impl, ExecutionPreflight,
};
use crate::runner::error::RunnerError;

pub(in crate::runner) use super::binding::ContainerExecutionBinding;
pub(in crate::runner) use super::binding::{
    ensure_inline_workspace_supported, resolve_execution_binding_resolution, ExecutionBindingKind,
    ExecutionBindingResolution, InlineWorkspaceCapabilitySurface,
};

pub(in crate::runner) fn resolve_container_execution_binding(
    default_run_in: Option<effigy_manifest::ManifestTaskRunIn>,
    systems: Option<&effigy_manifest::ManifestSystemsConfig>,
    containers: Option<&effigy_manifest::ManifestContainersConfig>,
    task_name: &str,
    task: &ManifestTask,
    runtime_surface: &str,
) -> Result<ContainerExecutionBinding, RunnerError> {
    resolve_container_execution_binding_impl(
        default_run_in,
        systems,
        containers,
        task_name,
        task,
        runtime_surface,
    )
}

pub(in crate::runner) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd_impl(task, cwd)
}

pub(in crate::runner) fn run_manifest_task_with_cwd_and_env(
    task: &TaskInvocation,
    cwd: PathBuf,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    run_manifest_task_with_cwd_and_env_impl(task, cwd, env_overrides)
}

pub(in crate::runner) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    build_execution_preflight_impl(task, cwd)
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
    super::pipeline::standard::run_standard_task(&preflight, &selection)
}
