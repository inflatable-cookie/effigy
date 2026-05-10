use effigy_runtime::data::{
    run_container_volume_list, run_container_volume_list_for_repo,
    run_container_volume_prune_for_repo, run_container_volume_prune_global,
};

use crate::runner::command_context::{active_invocation_cwd, resolve_active_command_context};
use crate::runner::error::RunnerError;

use super::data::maybe_confirm_destructive_container_action;
use super::runtime_volume_capture;

pub(super) fn run_container_volume_command(
    repo_override: Option<std::path::PathBuf>,
    subcommand: &effigy_cli::ContainerVolumeSubcommand,
    output_json: bool,
) -> Result<String, RunnerError> {
    match subcommand {
        effigy_cli::ContainerVolumeSubcommand::List {
            global: true,
            orphans,
            dormant: _,
        } => {
            if repo_override.is_some() {
                return Err(RunnerError::task_invocation(
                    "`effigy container volume list --global` does not accept `--repo`; use `effigy container volume list` for one repo or omit `--repo` for cross-runtime inventory",
                ));
            }
            let cwd = active_invocation_cwd()?;
            run_container_volume_list(&cwd, *orphans, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
        effigy_cli::ContainerVolumeSubcommand::List {
            global: false,
            orphans: _,
            dormant,
        } => {
            let context = resolve_active_command_context(repo_override)?;
            run_container_volume_list_for_repo(
                &context.resolved.resolved_root,
                *dormant,
                output_json,
                runtime_volume_capture,
            )
            .map_err(Into::into)
        }
        effigy_cli::ContainerVolumeSubcommand::Prune {
            global: true,
            yes,
            orphans: _,
            dormant: _,
        } => {
            if repo_override.is_some() {
                return Err(RunnerError::task_invocation(
                    "`effigy container volume prune --global` does not accept `--repo`; use `effigy container volume prune --dormant` for one repo or omit `--repo` for global orphan cleanup",
                ));
            }
            maybe_confirm_destructive_container_action(
                "`effigy container volume prune --global --orphans`",
                "Purge ownerless Effigy-managed volumes across available runtimes.",
                output_json,
                *yes,
            )?;
            let cwd = active_invocation_cwd()?;
            run_container_volume_prune_global(&cwd, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
        effigy_cli::ContainerVolumeSubcommand::Prune {
            global: false,
            yes,
            orphans: _,
            dormant: _,
        } => {
            maybe_confirm_destructive_container_action(
                "`effigy container volume prune --dormant`",
                "Purge dormant Effigy-managed volumes that the current repo no longer declares or mounts.",
                output_json,
                *yes,
            )?;
            let context = resolve_active_command_context(repo_override)?;
            run_container_volume_prune_for_repo(
                &context.resolved.resolved_root,
                output_json,
                runtime_volume_capture,
            )
            .map_err(Into::into)
        }
    }
}
