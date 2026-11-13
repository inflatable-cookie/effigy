use std::path::PathBuf;

use effigy_core::repo;

use crate::runner::error::RunnerError;

pub(in crate::runner) fn current_working_dir() -> Result<PathBuf, RunnerError> {
    if let Some(context) = super::active_runtime_context() {
        return Ok(context.invocation_cwd().to_path_buf());
    }
    std::env::current_dir().map_err(RunnerError::Cwd)
}

/// Resolve a cwd to the identity comparisons use. Callers that must know
/// whether the path resolved read [`repo::ResolvedPath::is_resolved`]; the rest
/// take the path, which is the original one when it could not be resolved.
pub(in crate::runner) fn canonicalize_or_original(path: &std::path::Path) -> repo::ResolvedPath {
    repo::canonicalize_or_original(path)
}
