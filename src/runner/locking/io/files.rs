use std::fs;
use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;

pub(super) fn read_lock_record(path: &Path) -> Result<super::LockRecord, RunnerError> {
    let body = fs::read(path).map_err(|error| RunnerError::TaskLockIo {
        path: path.to_path_buf(),
        error,
    })?;
    serde_json::from_slice::<super::LockRecord>(&body).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to parse lock record {}: {error}",
            path.display()
        ))
    })
}

pub(super) fn remove_lock_file(path: &Path) -> Result<(), RunnerError> {
    fs::remove_file(path).map_err(|error| task_lock_io(path.to_path_buf(), error))
}

pub(super) fn task_lock_io(path: PathBuf, error: std::io::Error) -> RunnerError {
    RunnerError::TaskLockIo { path, error }
}
