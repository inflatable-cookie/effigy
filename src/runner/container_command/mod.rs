//! CLI command handler for `effigy container` subcommands.

use effigy_containers::ContainerCommandReport;
use effigy_runtime::EffigyRuntimeError;

use crate::runner::command_context::resolve_active_command_context;
use effigy_cli::{ContainerArgs, ContainerSubcommand};

use super::error::RunnerError;
use cache::run_container_cache_command;
use data::run_container_data_command;
use lifecycle::{
    run_container_down_command, run_container_eject, run_container_logs_command,
    run_container_reset_command, run_container_shell, run_container_stats_command,
    run_container_status_command, run_container_up,
};
use volume::run_container_volume_command;

pub(in crate::runner) use closeout::maybe_confirm_container_shell_exit_cleanup;
pub(in crate::runner) use gateway_registration::{
    gateway_routes_registered_for_container, register_gateway_routes_for_container,
};
pub(in crate::runner) use lifecycle::{
    run_container_exec_capture_with_options, run_container_exec_operation_capture,
    run_container_reset_adapter,
};
pub(in crate::runner) use secret_env::container_secret_runtime_env_path;

mod cache;
mod closeout;
mod data;
mod gateway_registration;
mod lifecycle;
mod secret_env;
mod shell_prep;
pub(in crate::runner) mod support;
#[cfg(test)]
pub(super) mod test_support;
mod volume;

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(in crate::runner) fn run_container(args: ContainerArgs) -> Result<String, RunnerError> {
    match args.subcommand {
        ContainerSubcommand::Up {
            name,
            attach,
            detach,
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_up(
                &context.resolved.resolved_root,
                name.as_deref(),
                attach,
                detach,
                args.output_json,
            )
        }
        ContainerSubcommand::Down {
            name,
            global: false,
        } => run_container_down_command(
            args.repo_override.clone(),
            name.as_deref(),
            false,
            args.output_json,
        ),
        ContainerSubcommand::Down { name, global: true } => run_container_down_command(
            args.repo_override.clone(),
            name.as_deref(),
            true,
            args.output_json,
        ),
        ContainerSubcommand::Status {
            name,
            global: false,
        } => run_container_status_command(
            args.repo_override.clone(),
            name.as_deref(),
            false,
            args.output_json,
        ),
        ContainerSubcommand::Status { name, global: true } => run_container_status_command(
            args.repo_override.clone(),
            name.as_deref(),
            true,
            args.output_json,
        ),
        ContainerSubcommand::Stats { global: false } => unreachable!("parser rejects this shape"),
        ContainerSubcommand::Stats { global: true } => {
            run_container_stats_command(args.repo_override.clone(), true, args.output_json)
        }
        ContainerSubcommand::Logs {
            name,
            service,
            follow,
        } => run_container_logs_command(
            args.repo_override.clone(),
            name.as_deref(),
            service.as_deref(),
            follow,
            args.output_json,
        ),
        ContainerSubcommand::Shell {
            name,
            service,
            command,
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_shell(
                &context.resolved.resolved_root,
                name.as_deref(),
                service.as_deref(),
                command.as_deref(),
                args.output_json,
            )
        }
        ContainerSubcommand::Reset {
            name,
            keep_data,
            wipe_data,
            yes,
        } => run_container_reset_command(
            args.repo_override.clone(),
            name.as_deref(),
            keep_data,
            wipe_data,
            yes,
            args.output_json,
        ),
        ContainerSubcommand::Data { name, subcommand } => run_container_data_command(
            args.repo_override.clone(),
            name.as_deref(),
            &subcommand,
            args.output_json,
        ),
        ContainerSubcommand::Cache { name, subcommand } => run_container_cache_command(
            args.repo_override.clone(),
            name.as_deref(),
            &subcommand,
            args.output_json,
        ),
        ContainerSubcommand::Volume { subcommand } => {
            run_container_volume_command(args.repo_override.clone(), &subcommand, args.output_json)
        }
        ContainerSubcommand::Eject { name } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_eject(
                &context.resolved.resolved_root,
                name.as_deref(),
                args.output_json,
            )
        }
    }
}

fn deregister_runtime_gateway_routes(
    policy: &effigy_containers::EffectiveContainerPolicy,
) -> Result<Vec<String>, effigy_runtime::EffigyRuntimeError> {
    gateway_registration::deregister_gateway_routes_for_container(policy)
        .map_err(runtime_error_from_runner)
}

fn runtime_volume_capture(
    repo_root: &std::path::Path,
    profile: &str,
    command: &effigy_catalog::volumes::DockerCommand,
) -> Result<std::process::Output, effigy_runtime::EffigyRuntimeError> {
    support::run_runtime_volume_capture(repo_root, profile, command)
        .map_err(runtime_error_from_runner)
}

pub(in crate::runner) fn runtime_error_from_runner(error: RunnerError) -> EffigyRuntimeError {
    EffigyRuntimeError::task_invocation(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::container_command::support::repo_root_has_effigy_manifest;
    use effigy_cli::{ContainerSubcommand, ContainerVolumeSubcommand};

    #[test]
    fn container_stats_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Stats { global: true },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("stats --global should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container stats --global` does not accept `--repo`"));
    }

    #[test]
    fn container_status_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Status {
                name: None,
                global: true,
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("status --global should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container status --global` does not accept `--repo`"));
    }

    #[test]
    fn container_down_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
                name: None,
                global: true,
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("down --global should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container down --global` does not accept `--repo`"));
    }

    #[test]
    fn container_volume_list_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::List {
                    global: true,
                    orphans: false,
                    dormant: false,
                },
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("volume list --global should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container volume list --global` does not accept `--repo`"));
    }

    #[test]
    fn container_volume_prune_global_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Volume {
                subcommand: ContainerVolumeSubcommand::Prune {
                    global: true,
                    yes: true,
                    orphans: true,
                    dormant: false,
                },
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("volume prune --global should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container volume prune --global` does not accept `--repo`"));
    }

    #[test]
    fn repo_root_has_effigy_manifest_requires_real_manifest() {
        let temp = tempfile::tempdir().expect("tempdir");
        let plain = temp.path().join("plain");
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&plain).expect("mkdir plain");
        std::fs::create_dir_all(&repo).expect("mkdir repo");
        std::fs::write(repo.join("effigy.toml"), "[manifest]\n").expect("write manifest");

        assert!(!repo_root_has_effigy_manifest(&plain));
        assert!(repo_root_has_effigy_manifest(&repo));
    }
}
