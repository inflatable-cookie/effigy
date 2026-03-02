use std::path::PathBuf;

use crate::TaskInvocation;

use super::RunnerError;
#[cfg(test)]
use super::LoadedCatalog;

pub(super) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    super::execute::run_manifest_task_with_cwd(task, cwd)
}

#[cfg(test)]
pub(super) fn builtin_test_max_parallel(
    catalogs: &[LoadedCatalog],
    resolved_root: &std::path::Path,
) -> usize {
    super::builtin::builtin_test_max_parallel(catalogs, resolved_root)
}
