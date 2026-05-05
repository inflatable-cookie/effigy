//! CLI command handler for `effigy container` subcommands.

use effigy_containers::ContainerCommandReport;
use effigy_runtime::data::{
    run_container_data_export, run_container_data_import, run_container_data_list,
};
use effigy_runtime::read::{
    run_container_logs, run_container_stats_all, run_container_status, run_container_status_all,
};
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_reset,
};
use effigy_runtime::EffigyRuntimeError;

use crate::runner::command_context::resolve_active_command_context;
use effigy_cli::{ContainerArgs, ContainerDataSubcommand, ContainerSubcommand};

use super::error::RunnerError;
use data::{maybe_confirm_container_data_import, run_container_data_pull_production};
use lifecycle::{run_container_eject, run_container_shell, run_container_up};

pub(in crate::runner) use gateway_registration::{
    gateway_routes_registered_for_container, register_gateway_routes_for_container,
};
pub(in crate::runner) use lifecycle::{
    run_container_exec_capture, run_container_exec_capture_with_options,
};

mod data;
mod gateway_registration;
mod lifecycle;
pub(in crate::runner) mod support;

pub(super) fn render_container_report(report: ContainerCommandReport, output_json: bool) -> String {
    if output_json {
        report.json.to_string()
    } else {
        report.success_text
    }
}

pub(in crate::runner) fn run_container(args: ContainerArgs) -> Result<String, RunnerError> {
    if let ContainerSubcommand::Status { name: _, all: true } = &args.subcommand {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container status --all` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_status_all(args.output_json).map_err(Into::into);
    }
    if let ContainerSubcommand::Stats { all: true } = &args.subcommand {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container stats --all` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_stats_all(args.output_json).map_err(Into::into);
    }
    if let ContainerSubcommand::Down { name: _, all: true } = &args.subcommand {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container down --all` does not accept `--repo`; it discovers running environments across repos",
            ));
        }
        return run_container_down_all_with_hook(
            args.output_json,
            deregister_runtime_gateway_routes,
            |repo_root, policy| {
                let _ = super::host_process::stop_host_processes_for_container(repo_root, policy);
            },
        )
        .map_err(Into::into);
    }

    let context = resolve_active_command_context(args.repo_override.clone())?;
    let cwd = context.invocation_cwd;
    let repo_root = context.resolved.resolved_root;

    match args.subcommand {
        ContainerSubcommand::Up {
            name,
            attach,
            detach,
        } => run_container_up(
            &repo_root,
            name.as_deref(),
            attach,
            detach,
            args.output_json,
        ),
        ContainerSubcommand::Down { name, all: false } => {
            run_container_down_adapter(&repo_root, name.as_deref(), args.output_json)
        }
        ContainerSubcommand::Down { all: true, .. } => unreachable!("handled above"),
        ContainerSubcommand::Status { name, all: false } => {
            run_container_status(&repo_root, name.as_deref(), args.output_json).map_err(Into::into)
        }
        ContainerSubcommand::Status { all: true, .. } => unreachable!("handled above"),
        ContainerSubcommand::Stats { all: false } => unreachable!("parser rejects this shape"),
        ContainerSubcommand::Stats { all: true } => unreachable!("handled above"),
        ContainerSubcommand::Logs {
            name,
            service,
            follow,
        } => run_container_logs(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            follow,
            args.output_json,
        )
        .map_err(Into::into),
        ContainerSubcommand::Shell {
            name,
            service,
            command,
        } => run_container_shell(
            &repo_root,
            name.as_deref(),
            service.as_deref(),
            command.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Reset { name, keep_data } => {
            run_container_reset_adapter(&repo_root, name.as_deref(), keep_data, args.output_json)
        }
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::List,
        } => run_container_data_list_adapter(&repo_root, name.as_deref(), args.output_json),
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Export { volume, path },
        } => run_container_data_export_adapter(
            &repo_root,
            name.as_deref(),
            &volume,
            &resolve_archive_path(&cwd, &path),
            args.output_json,
        ),
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Import { volume, path, yes },
        } => run_container_data_import_adapter(
            &repo_root,
            name.as_deref(),
            &volume,
            &resolve_archive_path(&cwd, &path),
            args.output_json,
            yes,
        ),
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::PullProduction { yes },
        } => run_container_data_pull_production(&repo_root, name.as_deref(), args.output_json, yes),
        ContainerSubcommand::Eject { name } => {
            run_container_eject(&repo_root, name.as_deref(), args.output_json)
        }
    }
}

fn resolve_archive_path(cwd: &std::path::Path, path: &std::path::Path) -> std::path::PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn run_container_down_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    stop_host_processes_best_effort(repo_root, name);
    run_container_down(
        repo_root,
        name,
        output_json,
        deregister_runtime_gateway_routes,
    )
    .map_err(Into::into)
}

fn run_container_reset_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    keep_data: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    stop_host_processes_best_effort(repo_root, name);
    run_container_reset(
        repo_root,
        name,
        keep_data,
        output_json,
        deregister_runtime_gateway_routes,
        |repo_root, policy, classification| {
            support::remove_reset_volumes(repo_root, policy, classification)
                .map_err(runtime_error_from_runner)
        },
    )
    .map_err(Into::into)
}

/// Best-effort host-process shutdown. Runs before any compose-down so
/// supervisors stop spawning child processes. Failure to load the
/// policy (already gone, manifest deleted, etc.) is silently ignored.
fn stop_host_processes_best_effort(repo_root: &std::path::Path, name: Option<&str>) {
    if let Ok(policy) = effigy_containers::load_container_policy(repo_root, name) {
        let _ = super::host_process::stop_host_processes_for_container(repo_root, &policy);
    }
}

fn run_container_data_list_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_data_list(repo_root, name, output_json, runtime_volume_capture)
        .map_err(Into::into)
}

fn run_container_data_export_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &std::path::Path,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_data_export(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        runtime_volume_capture,
    )
    .map_err(Into::into)
}

fn run_container_data_import_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    volume_name: &str,
    archive_path: &std::path::Path,
    output_json: bool,
    yes: bool,
) -> Result<String, RunnerError> {
    maybe_confirm_container_data_import(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        yes,
    )?;
    run_container_data_import(
        repo_root,
        name,
        volume_name,
        archive_path,
        output_json,
        runtime_volume_capture,
    )
    .map_err(Into::into)
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
    use effigy_cli::ContainerSubcommand;

    #[test]
    fn container_stats_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Stats { all: true },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("stats --all should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container stats --all` does not accept `--repo`"));
    }

    #[test]
    fn container_down_all_rejects_repo_override() {
        let error = run_container(ContainerArgs {
            subcommand: ContainerSubcommand::Down {
                name: None,
                all: true,
            },
            repo_override: Some(std::path::PathBuf::from("/tmp/demo")),
            output_json: false,
        })
        .expect_err("down --all should reject --repo");

        assert!(error
            .to_string()
            .contains("`effigy container down --all` does not accept `--repo`"));
    }
}
