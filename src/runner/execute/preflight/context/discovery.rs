use std::path::PathBuf;

use effigy_core::resolver::ResolvedTarget;

use crate::runner::command_context::{canonicalize_or_original, resolve_repo_root};
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_routing::discover_catalogs_allow_missing;
use effigy_tasks::{parse_task_selector, TaskSelector};

pub(in crate::runner) struct ExecutionPreflightDiscovery {
    pub(in crate::runner) invocation_cwd: PathBuf,
    pub(in crate::runner) resolved: ResolvedTarget,
    pub(in crate::runner) selector: TaskSelector,
    pub(in crate::runner) catalogs: Vec<LoadedCatalog>,
}

pub(in crate::runner) fn discover_execution_preflight(
    task_name: &str,
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ExecutionPreflightDiscovery, RunnerError> {
    let invocation_cwd = canonicalize_or_original(&cwd);
    let resolved = resolve_repo_root(cwd, repo_override)?;
    let selector = parse_task_selector(task_name).map_err(RunnerError::task_invocation)?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    Ok(ExecutionPreflightDiscovery {
        invocation_cwd,
        resolved,
        selector,
        catalogs,
    })
}
