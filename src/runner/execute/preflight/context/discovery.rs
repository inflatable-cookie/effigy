use std::path::PathBuf;

use effigy_context::TaskSourceContext;
use effigy_core::resolver::ResolvedTarget;
use effigy_execution::{ExecutionDiscoveryInput, ExecutionDiscoveryPlan};

use crate::runner::command_context::resolve_command_context_from_cwd;
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;
use effigy_routing::{load_effective_catalogs_allow_missing, load_isolated_catalog};

pub(in crate::runner) struct ExecutionPreflightDiscovery {
    pub(in crate::runner) resolved: ResolvedTarget,
    pub(in crate::runner) plan: ExecutionDiscoveryPlan,
    pub(in crate::runner) catalogs: Vec<LoadedCatalog>,
}

pub(in crate::runner) fn discover_execution_preflight(
    task_name: &str,
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
    task_source: Option<&TaskSourceContext>,
) -> Result<ExecutionPreflightDiscovery, RunnerError> {
    let context = resolve_command_context_from_cwd(cwd, repo_override.clone())?;
    let plan =
        ExecutionDiscoveryInput::new(task_name, context.invocation_cwd.clone(), repo_override)
            .resolve(
                context.invocation_cwd.clone(),
                context.resolved.resolved_root.clone(),
            )
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let catalogs = if let Some(source) = task_source {
        vec![load_isolated_catalog(
            &source.source_root,
            &source.manifest_path,
        )?]
    } else {
        load_effective_catalogs_allow_missing(&context.resolved.resolved_root)?
    };
    Ok(ExecutionPreflightDiscovery {
        resolved: context.resolved,
        plan,
        catalogs,
    })
}
