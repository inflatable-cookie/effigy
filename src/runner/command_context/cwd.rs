use std::path::PathBuf;

use effigy_core::repo;

use crate::runner::error::RunnerError;

pub(in crate::runner) fn current_working_dir() -> Result<PathBuf, RunnerError> {
    repo::current_working_dir().map_err(RunnerError::Cwd)
}

pub(in crate::runner) fn canonicalize_or_original(path: &PathBuf) -> PathBuf {
    repo::canonicalize_or_original(path)
}
