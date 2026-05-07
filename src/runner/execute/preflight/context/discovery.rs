use std::path::PathBuf;

use effigy_core::resolver::ResolvedTarget;
use effigy_execution::{ExecutionDiscoveryInput, ExecutionDiscoveryPlan};

use crate::runner::command_context::resolve_command_context_from_cwd;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_routing::discover_catalogs_allow_missing;

pub(in crate::runner) struct ExecutionPreflightDiscovery {
    pub(in crate::runner) resolved: ResolvedTarget,
    pub(in crate::runner) plan: ExecutionDiscoveryPlan,
    pub(in crate::runner) catalogs: Vec<LoadedCatalog>,
}

pub(in crate::runner) fn discover_execution_preflight(
    task_name: &str,
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ExecutionPreflightDiscovery, RunnerError> {
    let context = resolve_command_context_from_cwd(cwd, repo_override.clone())?;
    let plan =
        ExecutionDiscoveryInput::new(task_name, context.invocation_cwd.clone(), repo_override)
            .resolve(
                context.invocation_cwd.clone(),
                context.resolved.resolved_root.clone(),
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let catalogs = discover_catalogs_allow_missing(&context.resolved.resolved_root)?;
    Ok(ExecutionPreflightDiscovery {
        resolved: context.resolved,
        plan,
        catalogs,
    })
}
