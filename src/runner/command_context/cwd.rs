use std::path::PathBuf;

use effigy_core::repo;

use crate::runner::error::RunnerError;

pub(in crate::runner) fn current_working_dir() -> Result<PathBuf, RunnerError> {
    if let Some(context) = super::active_runtime_context() {
        return Ok(context.invocation_cwd().to_path_buf());
    }
    repo::current_working_dir().map_err(RunnerError::Cwd)
}

pub(in crate::runner) fn canonicalize_or_original(path: &PathBuf) -> PathBuf {
    repo::canonicalize_or_original(path)
}
