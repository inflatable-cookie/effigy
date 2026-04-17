#[path = "context/discovery.rs"]
mod discovery;

use std::path::PathBuf;

use crate::TaskInvocation;

use super::runtime::prepare_execution_runtime_args;
use crate::resolver::ResolvedTarget;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

pub(in crate::runner) struct ExecutionPreflight {
    pub(in crate::runner) invocation_cwd: PathBuf,
    pub(in crate::runner) runtime_args_raw: TaskRuntimeArgs,
    pub(in crate::runner) runtime_args_exec: TaskRuntimeArgs,
    pub(in crate::runner) output_json: bool,
    pub(in crate::runner) resolved: ResolvedTarget,
    pub(in crate::runner) selector: TaskSelector,
    pub(in crate::runner) catalogs: Vec<LoadedCatalog>,
}

pub(in crate::runner) fn build_execution_preflight(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<ExecutionPreflight, RunnerError> {
    let (runtime_args_raw, runtime_args_exec, output_json) =
        prepare_execution_runtime_args(&task.args)?;
    let discovery = discovery::discover_execution_preflight(
        &task.name,
        cwd,
        runtime_args_raw.repo_override.clone(),
    )?;
    Ok(ExecutionPreflight {
        invocation_cwd: discovery.invocation_cwd,
        runtime_args_raw,
        runtime_args_exec,
        output_json,
        resolved: discovery.resolved,
        selector: discovery.selector,
        catalogs: discovery.catalogs,
    })
}
