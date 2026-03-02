use std::fs;
use std::path::{Path, PathBuf};

use super::super::super::RunnerError;

pub(super) fn ensure_locks_root(workspace_root: &Path) -> Result<PathBuf, RunnerError> {
    let locks_root = workspace_root.join(super::LOCKS_DIR);
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;
    Ok(locks_root)
}
