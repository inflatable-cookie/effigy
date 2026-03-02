#[path = "locking/io.rs"]
mod io;
#[path = "locking/model.rs"]
mod model;

pub(super) type LockScope = model::LockScope;
pub(super) type LockGuard = io::LockGuard;
pub(super) type UnlockResult = io::UnlockResult;

pub(super) fn acquire_scopes(
    workspace_root: &std::path::Path,
    scopes: &[LockScope],
) -> Result<Vec<LockGuard>, super::RunnerError> {
    io::acquire_scopes(workspace_root, scopes)
}

pub(super) fn unlock_scopes(
    workspace_root: &std::path::Path,
    scopes: &[LockScope],
) -> Result<UnlockResult, super::RunnerError> {
    io::unlock_scopes(workspace_root, scopes)
}

pub(super) fn unlock_all(
    workspace_root: &std::path::Path,
) -> Result<UnlockResult, super::RunnerError> {
    io::unlock_all(workspace_root)
}
