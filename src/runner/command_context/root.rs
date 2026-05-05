use std::path::PathBuf;

use effigy_cli::Command;
use effigy_core::resolver::{resolve_target_root, ResolvedTarget};

use crate::runner::error::RunnerError;

pub(in crate::runner) fn resolve_repo_root(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ResolvedTarget, RunnerError> {
    if let Some(context) = super::active_runtime_context() {
        let cwd_matches = cwd == context.invocation_cwd();
        let override_matches = match (repo_override.as_deref(), context.repo_override()) {
            (None, None) => true,
            (Some(requested), Some(active)) => canonicalize_or_original(requested) == active,
            _ => false,
        };
        if cwd_matches && override_matches {
            return Ok(context.resolved_target().clone());
        }
    }
    resolve_target_root(cwd, repo_override).map_err(RunnerError::Resolve)
}

pub(in crate::runner) fn resolve_command_root(cmd: &Command) -> PathBuf {
    if let Some(context) = super::active_runtime_context() {
        return context.command_root().to_path_buf();
    }
    let cwd = super::current_working_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_override = super::command_repo_override(cmd);

    match resolve_repo_root(cwd.clone(), repo_override) {
        Ok(resolved) => resolved.resolved_root,
        Err(_) => cwd,
    }
}

fn canonicalize_or_original(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
