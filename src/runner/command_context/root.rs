use std::path::PathBuf;

use effigy_cli::Command;
use effigy_core::resolver::{resolve_target_root, ResolvedTarget};

use crate::runner::error::RunnerError;

pub(in crate::runner) fn resolve_repo_root(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
) -> Result<ResolvedTarget, RunnerError> {
    resolve_target_root(cwd, repo_override).map_err(RunnerError::Resolve)
}

pub(in crate::runner) fn resolve_command_root(cmd: &Command) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_override = super::command_repo_override(cmd);

    match resolve_repo_root(cwd.clone(), repo_override) {
        Ok(resolved) => resolved.resolved_root,
        Err(_) => cwd,
    }
}
