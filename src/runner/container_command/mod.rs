//! CLI command handler for `effigy container` subcommands.

use effigy_containers::ContainerCommandReport;
use effigy_runtime::data::{
    run_container_cache_list, run_container_cache_list_all, run_container_cache_list_under_path,
    run_container_cache_prune, run_container_cache_prune_all, run_container_data_export,
    run_container_data_import, run_container_data_list,
};
use effigy_runtime::read::{
    run_container_logs, run_container_stats_all, run_container_status, run_container_status_all,
    run_container_status_under_path,
};
use effigy_runtime::write::{
    run_container_down, run_container_down_all_with_hook, run_container_down_under_path_with_hook,
    run_container_reset,
};
use effigy_runtime::EffigyRuntimeError;

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::db_seed::resolve_db_seed_input_paths;
use effigy_cli::{
    ContainerArgs, ContainerCacheSubcommand, ContainerDataSubcommand, ContainerSubcommand,
};

use super::error::RunnerError;
use data::{
    maybe_confirm_container_data_import, resolve_db_dump_output_paths, run_container_data_dump,
    run_container_data_pull_production, run_container_data_seed,
};
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
    if let ContainerSubcommand::Cache {
        name: _,
        subcommand:
            ContainerCacheSubcommand::List {
                all: true,
                project,
                kind,
            },
    } = &args.subcommand
    {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container cache list --all` does not accept `--repo`; it inspects the Effigy Colima profile's named-volume inventory",
            ));
        }
        let cwd = crate::runner::command_context::active_invocation_cwd()?;
        return run_container_cache_list_all(
            &cwd,
            None,
            project.as_deref(),
            kind.as_deref(),
            args.output_json,
            runtime_volume_capture,
        )
        .map_err(Into::into);
    }
    if let ContainerSubcommand::Cache {
        name: _,
        subcommand:
            ContainerCacheSubcommand::Prune {
                all: true,
                yes,
                project,
                kind,
            },
    } = &args.subcommand
    {
        if args.repo_override.is_some() {
            return Err(RunnerError::task_invocation(
                "`effigy container cache prune --all` does not accept `--repo`; it prunes cache volumes from the Effigy Colima profile inventory",
            ));
        }
        data::maybe_confirm_destructive_container_action(
            "`effigy container cache prune --all`",
            "Purge safe cache volumes across the Effigy Colima profile. Running projects will be skipped.",
            args.output_json,
            *yes,
        )?;
        let cwd = crate::runner::command_context::active_invocation_cwd()?;
        return run_container_cache_prune_all(
            &cwd,
            None,
            project.as_deref(),
            kind.as_deref(),
            args.output_json,
            runtime_volume_capture,
        )
        .map_err(Into::into);
    }
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
        ContainerSubcommand::Down { name, all: false } => run_container_down_fallback(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Down { all: true, .. } => unreachable!("handled above"),
        ContainerSubcommand::Status { name, all: false } => run_container_status_fallback(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Status { all: true, .. } => unreachable!("handled above"),
        ContainerSubcommand::Stats { all: false } => unreachable!("parser rejects this shape"),
        ContainerSubcommand::Stats { all: true } => unreachable!("handled above"),
        ContainerSubcommand::Logs {
            name,
            service,
            follow,
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_logs(
                &context.resolved.resolved_root,
                name.as_deref(),
                service.as_deref(),
                follow,
                args.output_json,
            )
            .map_err(Into::into)
        }
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
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_reset_adapter(
                &context.resolved.resolved_root,
                name.as_deref(),
                keep_data,
                wipe_data,
                yes,
                args.output_json,
            )
        }
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::List,
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_list_adapter(
                &context.resolved.resolved_root,
                name.as_deref(),
                args.output_json,
            )
        }
        ContainerSubcommand::Cache {
            name,
            subcommand:
                ContainerCacheSubcommand::List {
                    all: false,
                    project: _,
                    kind: _,
                },
        } => run_container_cache_list_fallback(
            args.repo_override.clone(),
            name.as_deref(),
            args.output_json,
        ),
        ContainerSubcommand::Cache {
            subcommand:
                ContainerCacheSubcommand::List {
                    all: true,
                    project: _,
                    kind: _,
                },
            ..
        } => unreachable!("handled above"),
        ContainerSubcommand::Cache {
            name,
            subcommand:
                ContainerCacheSubcommand::Prune {
                    all: false,
                    yes,
                    project: _,
                    kind: _,
                },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            let repo_root = &context.resolved.resolved_root;
            let policy = effigy_containers::load_container_policy(repo_root, name.as_deref())?;
            data::maybe_confirm_destructive_container_action(
                &format!("`effigy container {} cache prune`", policy.name),
                &format!(
                    "Purge safe cache volumes for container `{}`. The container must be stopped first.",
                    policy.name
                ),
                args.output_json,
                yes,
            )?;
            run_container_cache_prune(
                repo_root,
                name.as_deref(),
                args.output_json,
                runtime_volume_capture,
            )
            .map_err(Into::into)
        }
        ContainerSubcommand::Cache {
            subcommand:
                ContainerCacheSubcommand::Prune {
                    all: true,
                    yes: _,
                    project: _,
                    kind: _,
                },
            ..
        } => unreachable!("handled above"),
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Export { volume, path },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_export_adapter(
                &context.resolved.resolved_root,
                name.as_deref(),
                &volume,
                &resolve_archive_path(&context.invocation_cwd, &path),
                args.output_json,
            )
        }
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Dump { db_dumps, push },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_dump(
                &context.resolved.resolved_root,
                name.as_deref(),
                &resolve_db_dump_output_paths(&context.invocation_cwd, &db_dumps),
                push,
                args.output_json,
            )
        }
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Import { volume, path, yes },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_import_adapter(
                &context.resolved.resolved_root,
                name.as_deref(),
                &volume,
                &resolve_archive_path(&context.invocation_cwd, &path),
                args.output_json,
                yes,
            )
        }
        ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::PullProduction { yes },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_pull_production(
                &context.resolved.resolved_root,
                name.as_deref(),
                args.output_json,
                yes,
            )
        }
        ContainerSubcommand::Data {
            name,
            subcommand:
                ContainerDataSubcommand::Seed {
                    db_seeds,
                    no_prompt,
                    yes,
                },
        } => {
            let context = resolve_active_command_context(args.repo_override.clone())?;
            run_container_data_seed(
                &context.resolved.resolved_root,
                name.as_deref(),
                &resolve_db_seed_input_paths(&context.invocation_cwd, &db_seeds),
                args.output_json,
                no_prompt,
                yes,
            )
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

fn run_container_status_fallback(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) if repo_root_has_effigy_manifest(&context.resolved.resolved_root) => {
            run_container_status(&context.resolved.resolved_root, name, output_json)
                .map_err(Into::into)
        }
        Ok(_) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_status_under_path(&cwd, name, output_json).map_err(Into::into)
        }
        Ok(context) => Err(RunnerError::task_invocation(format!(
            "`--repo {}` does not point to an Effigy repo",
            context.resolved.resolved_root.display()
        ))),
        Err(RunnerError::Resolve(_)) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_status_under_path(&cwd, name, output_json).map_err(Into::into)
        }
        Err(error) => Err(error),
    }
}

fn run_container_down_fallback(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) if repo_root_has_effigy_manifest(&context.resolved.resolved_root) => {
            run_container_down_adapter(&context.resolved.resolved_root, name, output_json)
        }
        Ok(_) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_down_under_path_with_hook(
                &cwd,
                name,
                output_json,
                deregister_runtime_gateway_routes,
                |repo_root, policy| {
                    let _ =
                        super::host_process::stop_host_processes_for_container(repo_root, policy);
                },
            )
            .map_err(Into::into)
        }
        Ok(context) => Err(RunnerError::task_invocation(format!(
            "`--repo {}` does not point to an Effigy repo",
            context.resolved.resolved_root.display()
        ))),
        Err(RunnerError::Resolve(_)) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_down_under_path_with_hook(
                &cwd,
                name,
                output_json,
                deregister_runtime_gateway_routes,
                |repo_root, policy| {
                    let _ =
                        super::host_process::stop_host_processes_for_container(repo_root, policy);
                },
            )
            .map_err(Into::into)
        }
        Err(error) => Err(error),
    }
}

fn repo_root_has_effigy_manifest(repo_root: &std::path::Path) -> bool {
    repo_root.join("effigy.toml").is_file()
}

pub(in crate::runner) fn run_container_reset_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    keep_data: bool,
    wipe_data: bool,
    yes: bool,
    output_json: bool,
) -> Result<String, RunnerError> {
    stop_host_processes_best_effort(repo_root, name);
    maybe_confirm_container_reset_wipe_data(repo_root, name, output_json, wipe_data, yes)?;
    run_container_reset(
        repo_root,
        name,
        keep_data,
        wipe_data,
        output_json,
        deregister_runtime_gateway_routes,
        |repo_root, policy, classification| {
            support::remove_reset_volumes(repo_root, policy, classification)
                .map_err(runtime_error_from_runner)
        },
    )
    .map_err(Into::into)
}

fn maybe_confirm_container_reset_wipe_data(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
    wipe_data: bool,
    yes: bool,
) -> Result<(), RunnerError> {
    if !wipe_data {
        return Ok(());
    }
    let policy = effigy_containers::load_container_policy(repo_root, name)?;
    data::maybe_confirm_destructive_container_action(
        &format!("`effigy container {} reset --wipe-data`", policy.name),
        &format!(
            "Reset container `{}` and delete persistent generated-compose data volumes.",
            policy.name
        ),
        output_json,
        yes,
    )
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

fn run_container_cache_list_adapter(
    repo_root: &std::path::Path,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    run_container_cache_list(repo_root, name, output_json, runtime_volume_capture)
        .map_err(Into::into)
}

fn run_container_cache_list_fallback(
    repo_override: Option<std::path::PathBuf>,
    name: Option<&str>,
    output_json: bool,
) -> Result<String, RunnerError> {
    match resolve_active_command_context(repo_override.clone()) {
        Ok(context) if repo_root_has_effigy_manifest(&context.resolved.resolved_root) => {
            run_container_cache_list_adapter(&context.resolved.resolved_root, name, output_json)
        }
        Ok(_) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_cache_list_under_path(&cwd, name, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
        Ok(context) => Err(RunnerError::task_invocation(format!(
            "`--repo {}` does not point to an Effigy repo",
            context.resolved.resolved_root.display()
        ))),
        Err(RunnerError::Resolve(_)) if repo_override.is_none() => {
            let cwd = crate::runner::command_context::active_invocation_cwd()?;
            run_container_cache_list_under_path(&cwd, name, output_json, runtime_volume_capture)
                .map_err(Into::into)
        }
        Err(error) => Err(error),
    }
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
