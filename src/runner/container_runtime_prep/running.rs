use std::path::{Path, PathBuf};

use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;
use crate::runner::system_command::is_primary_service_running;

pub(super) fn check_runtime_running_state_stage(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<bool, RunnerError> {
    is_primary_service_running(repo_root, policy)
}

pub(super) fn ensure_runtime_running_stage(
    system_was_running: bool,
    container_name: Option<String>,
    repo_override: Option<PathBuf>,
    run_container: impl FnOnce(ContainerArgs) -> Result<String, RunnerError>,
) -> Result<(), RunnerError> {
    if system_was_running {
        return Ok(());
    }

    run_container(ContainerArgs {
        subcommand: ContainerSubcommand::Up {
            name: container_name,
            attach: false,
            detach: true,
        },
        repo_override,
        output_json: false,
    })?;
    Ok(())
}
