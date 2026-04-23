use std::fs;
use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;

pub(super) fn ensure_locks_root(workspace_root: &Path) -> Result<PathBuf, RunnerError> {
    let locks_root = workspace_root.join(super::LOCKS_DIR);
    ensure_effigy_ignored_in_git_root(workspace_root).map_err(|error| RunnerError::TaskLockIo {
        path: workspace_root.join(".gitignore"),
        error,
    })?;
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;
    Ok(locks_root)
}
