#[path = "context/discovery.rs"]
mod discovery;

use std::path::PathBuf;

use effigy_cli::TaskInvocation;
use effigy_execution::{ExecutionDiscoveryPlan, ExecutionPreflightInput, ExecutionSurface};

use super::runtime::prepare_execution_runtime_args;
use crate::runner::error::RunnerError;
use effigy_core::resolver::ResolvedTarget;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

pub(in crate::runner) struct ExecutionPreflight {
    pub(in crate::runner) invocation_cwd: PathBuf,
    pub(in crate::runner) execution_surface: ExecutionSurface,
    pub(in crate::runner) runtime_args_raw: TaskRuntimeArgs,
    pub(in crate::runner) runtime_args_exec: TaskRuntimeArgs,
    pub(in crate::runner) output_json: bool,
    pub(in crate::runner) resolved: ResolvedTarget,
    pub(in crate::runner) discovery_plan: ExecutionDiscoveryPlan,
    pub(in crate::runner) selector: TaskSelector,
    pub(in crate::runner) catalogs: Vec<LoadedCatalog>,
    pub(in crate::runner) secret_targets: Vec<String>,
}

pub(in crate::runner) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    build_execution_preflight_from_input(ExecutionPreflightInput::new(
        task.name.clone(),
        task.args.clone(),
        cwd,
        ExecutionSurface::DirectCli,
    ))
}

pub(in crate::runner) fn build_execution_preflight_from_input(
    input: ExecutionPreflightInput,
) -> Result<ExecutionPreflight, RunnerError> {
    let (runtime_args_raw, runtime_args_exec, output_json) =
        prepare_execution_runtime_args(&input.args)?;
    let discovery = discovery::discover_execution_preflight(
        &input.selector,
        input.cwd,
        runtime_args_raw.repo_override.clone(),
    )?;
    let discovery_plan = discovery.plan;
    Ok(ExecutionPreflight {
        invocation_cwd: discovery_plan.invocation_cwd.clone(),
        execution_surface: input.surface,
        runtime_args_raw,
        runtime_args_exec,
        output_json,
        resolved: discovery.resolved,
        selector: discovery_plan.selector.clone(),
        discovery_plan,
        catalogs: discovery.catalogs,
        secret_targets: input.secret_targets,
    })
}
