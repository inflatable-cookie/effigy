//! Which checkout a container command acts on.

use std::path::{Path, PathBuf};

use effigy_core::repo_markers::has_task_manifest;

use crate::runner::command_context::{active_invocation_cwd, resolve_active_command_context};
use crate::runner::container_command::RunnerError;

/// Which checkout the command acts on, when it acts on one at all.
pub(in crate::runner::container_command) enum ContainerRepoScope {
    RepoRoot(PathBuf),
    InvocationCwd(PathBuf),
}

pub(in crate::runner::container_command) fn resolve_repo_root_or_invocation_cwd_scope(
    repo_override: Option<PathBuf>,
) -> Result<ContainerRepoScope, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) if repo_root_has_effigy_manifest(&context.resolved.resolved_root) => Ok(
            ContainerRepoScope::RepoRoot(context.resolved.resolved_root.clone()),
        ),
        Ok(_) if repo_override.is_none() => {
            Ok(ContainerRepoScope::InvocationCwd(active_invocation_cwd()?))
        }
        Ok(context) => Err(RunnerError::task_invocation(format!(
            "`--repo {}` does not point to an Effigy repo",
            context.resolved.resolved_root.display()
        ))),
        Err(RunnerError::Resolve(_)) if repo_override.is_none() => {
            Ok(ContainerRepoScope::InvocationCwd(active_invocation_cwd()?))
        }
        Err(error) => Err(error),
    }
}

pub(in crate::runner::container_command) fn repo_root_has_effigy_manifest(
    repo_root: &Path,
) -> bool {
    has_task_manifest(repo_root)
}
