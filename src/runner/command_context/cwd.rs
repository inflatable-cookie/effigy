use std::path::PathBuf;

use crate::runner::error::RunnerError;

pub(in crate::runner) fn current_working_dir() -> Result<PathBuf, RunnerError> {
    std::env::current_dir().map_err(RunnerError::Cwd)
}

pub(in crate::runner) fn canonicalize_or_original(path: &PathBuf) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or(path.clone())
}
