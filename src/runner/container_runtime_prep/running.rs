use std::path::{Path, PathBuf};

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::container_command::is_primary_service_running;
use crate::runner::error::RunnerError;

pub(super) fn check_runtime_running_state_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    is_primary_service_running(repo_root, policy)
}

/// A stack that is not running is brought up detached as the task's ensure.
pub(super) fn ensure_runtime_running_stage(
    system_was_running: bool,
    container_name: Option<String>,
    repo_override: Option<PathBuf>,
    bring_up: impl FnOnce(Option<String>, Option<PathBuf>) -> Result<String, RunnerError>,
) -> Result<(), RunnerError> {
    if system_was_running {
        return Ok(());
    }
    bring_up(container_name, repo_override)?;
    Ok(())
}
