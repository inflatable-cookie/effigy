//! CLI command handler for `effigy container` subcommands.

use effigy_containers::ContainerCommandReport;
use effigy_runtime::EffigyRuntimeError;

use crate::runner::command_context::resolve_active_command_context;
use effigy_cli::{ContainerArgs, ContainerSubcommand};
use effigy_containers::environment::Retention;
use effigy_kernel::id::RunId;

use super::error::RunnerError;
use cache::run_container_cache_command;
use data::run_container_data_command;
use lifecycle::{
    run_container_down_command, run_container_logs_command, run_container_reset_command,
    run_container_shell, run_container_stats_command, run_container_status_command,
    run_container_up,
};
use profile::run_container_profile_command;
use volume::run_container_volume_command;

pub(in crate::runner) use bounded_exec::{
    bounded_failure_error, bounded_phase_error, run_container_shell_bounded,
};
pub(in crate::runner) use closeout::maybe_confirm_container_shell_exit_cleanup;
pub(in crate::runner) use gateway_registration::{
    gateway_routes_registered_for_container, register_gateway_routes_for_container,
};
pub(in crate::runner) use hosts::scoped_host_map;
pub(in crate::runner) use lifecycle::{
    release_managed_environment, run_container_exec_capture_with_credentials,
    run_container_exec_operation_capture, run_container_exec_stream,
};
pub(in crate::runner) use lifecycle::{WorkspacePreparedEffects, WorkspaceUpEffects};
pub(in crate::runner) use liveness::{
    is_primary_service_running, is_primary_service_running_with_deadline,
};
pub(in crate::runner) use secret_env::container_secret_runtime_env_path;
pub(in crate::runner) use shared_data::{
    resolve_shared_data_binding, run_shared_data_command, verify_shared_data_target,
};

mod bounded_exec;
mod cache;
mod closeout;
mod data;
pub(in crate::runner) mod data_credentials;
#[cfg(test)]
#[path = "data_custody_tests.rs"]
mod data_custody_tests;
mod gateway_registration;
mod handover;
mod hosts;
mod lifecycle;
mod liveness;
mod migrate;
pub(in crate::runner) mod policy;
mod profile;
mod readiness;
mod render;
pub(in crate::runner) mod reserved_input;
mod restart;
mod retire;
mod scope;
pub(in crate::runner) mod secret_env;
mod shared_credentials;
mod shared_data;
mod shell_prep;
mod steps;
pub(in crate::runner) mod support;
#[cfg(test)]
pub(super) mod test_support;
mod volume;
pub(in crate::runner) mod workspace_artifact_builder;
pub(in crate::runner) mod workspace_integrations;

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

/// CLI entry for `effigy container`. An explicit `up` owns its
/// environment persistently: no run's end idle-stops it until `down`.
/// Task activation brings environments up through [`run_container_up_held`].
pub(in crate::runner) fn run_container_command(args: ContainerArgs) -> Result<String, RunnerError> {
    run_container(args)
}

/// A task's ensure: the environment it starts is held, and `holders` are
/// attached by its activation so no idle stop can intervene.
pub(in crate::runner) fn run_container_up_held(
    repo_override: Option<std::path::PathBuf>,
    name: Option<String>,
    retention: Retention,
    holders: &[RunId],
) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(repo_override)?;
    run_container_up(
        &context.resolved.resolved_root,
        name.as_deref(),
        false,
        true,
        false,
        retention,
        holders,
    )
}

pub(in crate::runner) fn run_container(args: ContainerArgs) -> Result<String, RunnerError> {
    // A managed run's own ensure names its run as the holder: that is a
    // task's ensure, so a stack it starts is held (contract 053 §5). An
    // ensure never demotes a persistent stack.
    let retention = if crate::runner::environment::requested_holder().is_some() {
        crate::runner::environment::task_retention()
    } else {
        Retention::Persistent
    };
    run_container_with(args, retention, &[])
}

fn run_container_with(
    args: ContainerArgs,
    retention: Retention,
    holders: &[RunId],
) -> Result<String, RunnerError> {
    match args.subcommand {
        ContainerSubcommand::Up {
            name,
            system,
            attach,
            detach,
        } => {
            let name =
                resolve_selected_container_name(&args.repo_override, name, system, "container up")?;
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_up(
                &context.resolved.resolved_root,
                name.as_deref(),
                attach,
                detach,
                args.output_json,
                retention,
                holders,
            )
        }
        ContainerSubcommand::Down {
            name,
            system,
            global: false,
        } => {
            let name = resolve_selected_container_name(
                &args.repo_override,
                name,
                system,
                "container down",
            )?;
            run_container_down_command(
                args.repo_override.clone(),
                name.as_deref(),
                false,
                args.output_json,
            )
        }
        ContainerSubcommand::Down {
            name,
            system: _,
            global: true,
        } => run_container_down_command(
            args.repo_override.clone(),
            name.as_deref(),
            true,
            args.output_json,
        ),
        ContainerSubcommand::Status {
            name,
            system,
            global: false,
        } => {
            let name = resolve_selected_container_name(
                &args.repo_override,
                name,
                system,
                "container status",
            )?;
            run_container_status_command(
                args.repo_override.clone(),
                name.as_deref(),
                false,
                args.output_json,
            )
        }
        ContainerSubcommand::Status {
            name,
            system: _,
            global: true,
        } => run_container_status_command(
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
            system,
            service,
            follow,
        } => {
            let name = resolve_selected_container_name(
                &args.repo_override,
                name,
                system,
                "container logs",
            )?;
            run_container_logs_command(
                args.repo_override.clone(),
                name.as_deref(),
                service.as_deref(),
                follow,
                args.output_json,
            )
        }
        ContainerSubcommand::Restart { name, processes } => restart::run_container_restart(
            args.repo_override.clone(),
            name.as_deref(),
            &processes,
            args.output_json,
        ),
        ContainerSubcommand::Shell {
            name,
            service,
            command,
            bounded,
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            match bounded {
                Some(limits) => {
                    let command = command.as_deref().ok_or_else(|| {
                        RunnerError::task_invocation(
                            "`effigy container <NAME> shell --bounded` requires `--command <CMD>`",
                        )
                    })?;
                    run_container_shell_bounded(
                        &context.resolved.resolved_root,
                        name.as_deref(),
                        service.as_deref(),
                        command,
                        limits,
                        args.output_json,
                    )
                }
                None => run_container_shell(
                    &context.resolved.resolved_root,
                    name.as_deref(),
                    service.as_deref(),
                    command.as_deref(),
                    args.output_json,
                ),
            }
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
        ContainerSubcommand::Profile { subcommand } => {
            run_container_profile_command(args.repo_override.clone(), &subcommand, args.output_json)
        }
        ContainerSubcommand::Render { name } => render::run(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Migrate { name } => migrate::run(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Hosts { name } => hosts::run_container_hosts(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Scope => {
            scope::run_container_scope(args.repo_override.clone(), args.output_json)
        }
        ContainerSubcommand::Retire {
            name,
            scope,
            dead,
            keep_data,
            yes,
        } => {
            if dead {
                if name.is_some() {
                    return Err(RunnerError::task_invocation(
                        "`effigy container <NAME> retire --dead` is unsupported; `--dead` covers every labelled stack whose checkout is gone",
                    ));
                }
                retire::run_container_retire_dead(
                    args.repo_override.clone(),
                    keep_data,
                    yes,
                    args.output_json,
                )
            } else {
                retire::run_container_retire(
                    args.repo_override.clone(),
                    name.as_deref(),
                    scope.as_deref(),
                    keep_data,
                    yes,
                    args.output_json,
                )
            }
        }
    }
}

/// Resolve `--system <NAME>` to the container its default workspace binds.
/// A named container and a system selection cannot be combined.
fn resolve_selected_container_name(
    repo_override: &Option<std::path::PathBuf>,
    name: Option<String>,
    system: Option<String>,
    surface: &str,
) -> Result<Option<String>, RunnerError> {
    let Some(system) = system else {
        return Ok(name);
    };
    if let Some(name) = name {
        return Err(RunnerError::task_invocation(format!(
            "`effigy container {name} ...` does not accept `--system`; the container is already named"
        )));
    }
    let context = resolve_active_command_context(repo_override.clone())?;
    crate::runner::workspace_command::resolve_public_workspace_container(
        &context.resolved.resolved_root,
        Some(&system),
        None,
        surface,
    )
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
                system: None,
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
                system: None,
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
    fn system_selection_resolves_the_default_workspace_container() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        std::fs::create_dir_all(root.join("infra/dev")).expect("mkdir");
        std::fs::write(
            root.join("effigy.toml"),
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"

[containers.web]
primary_service = "app"
working_dir = "/workspace"

[containers.web.services.app]
catalog = "workspace-rust-bun"
"#,
        )
        .expect("manifest");

        let selected = resolve_selected_container_name(
            &Some(root.clone()),
            None,
            Some("dev".to_owned()),
            "container up",
        )
        .expect("system selection resolves");

        assert_eq!(selected.as_deref(), Some("web"));
        let direct = resolve_selected_container_name(
            &Some(root),
            Some("api".to_owned()),
            None,
            "container up",
        )
        .expect("a direct name needs no resolution");
        assert_eq!(direct.as_deref(), Some("api"));
    }

    #[test]
    fn named_container_refuses_system_selection_before_resolution() {
        let error = resolve_selected_container_name(
            &None,
            Some("web".to_owned()),
            Some("dev".to_owned()),
            "container up",
        )
        .expect_err("both selectors cannot be combined");

        assert!(error.to_string().contains("does not accept `--system`"));
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
