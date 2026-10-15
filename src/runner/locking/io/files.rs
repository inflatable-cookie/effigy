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

/// A private sibling for staging lock bytes before an atomic link or rename.
/// Its `.tmp` extension keeps it out of `.lock` listings.
pub(super) fn staging_path(path: &Path) -> PathBuf {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    path.with_file_name(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ))
}

pub(super) fn remove_lock_file(path: &Path) -> Result<(), RunnerError> {
    fs::remove_file(path).map_err(|error| task_lock_io(path.to_path_buf(), error))
}

pub(super) fn task_lock_io(path: PathBuf, error: std::io::Error) -> RunnerError {
    RunnerError::TaskLockIo { path, error }
}
