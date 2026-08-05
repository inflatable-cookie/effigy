use std::path::PathBuf;

#[path = "command_parsing_artifact.rs"]
mod artifact;
#[path = "command_parsing_bootstrap.rs"]
mod bootstrap;
#[path = "command_parsing_changelog.rs"]
mod changelog;
#[path = "command_parsing_container.rs"]
mod container;
#[path = "command_parsing_container_data.rs"]
mod container_data;
#[path = "command_parsing_demo.rs"]
mod demo;
#[path = "command_parsing_deploy.rs"]
mod deploy;
#[path = "command_parsing_docs.rs"]
mod docs;
#[path = "command_parsing_graph.rs"]
mod graph;
#[path = "command_parsing_release.rs"]
mod release;
#[path = "command_parsing_runtime.rs"]
mod runtime;
#[path = "command_parsing_secrets.rs"]
mod secrets;
#[path = "command_parsing_state.rs"]
mod state;

use crate::{
    BundleArgs, BundleSubcommand, CatalogArgs, CatalogCacheSubcommand, CatalogSubcommand, Command,
    ContractsArgs, ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand, DeferArgs,
    DepsArgs, DepsManager, DepsSubcommand, DoctorArgs, HelpTopic, InternalContainerLeaseReaperArgs,
    InternalGatewayArgs, InternalHostProcessStopArgs, InternalHostProcessSuperviseArgs,
    InternalScriptRunArgs, RhaiArgs, RhaiSubcommand, TaskInvocation, TasksArgs, UninstallArgs,
};
use artifact::parse_artifact_command;
use bootstrap::parse_bootstrap_command;
use changelog::parse_changelog_command;
use container::parse_container_command;
use demo::parse_demo_command;
use deploy::parse_deploy_command;
use docs::parse_docs_command;
use graph::parse_graph_command;
use release::parse_release_command;
use runtime::{
    parse_exec_command, parse_gateway_command, parse_service_command, parse_system_command,
    parse_workspace_command,
};
use secrets::parse_secrets_command;
use state::parse_state_command;

use super::value_parsing::{next_required_value, parse_pretty_bool, parse_repo_path};
use super::{unknown_argument, CliParseError};

pub(super) fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(cmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::General));
    };

    match cmd.as_str() {
        "--version" | "version" => parse_version_command(args),
        "--help" | "-h" | "help" => Ok(Command::Help(HelpTopic::General)),
        "bundle" => parse_bundle_command(args),
        "catalog" => parse_catalog_command(args),
        "changelog" => parse_changelog_command(args),
        "deploy" => parse_deploy_command(args),
        "deps" => parse_deps_command(args),
        "secrets" => parse_secrets_command(args),
        "defer" => parse_defer_command(args),
        "exec" => parse_exec_command(args),
        "state" => parse_state_command(args),
        "system" => parse_system_command(args),
        "workspace" => parse_workspace_command(args),
        "gateway" => parse_gateway_command(args),
        "service" => parse_service_command(args),
        "demo" => parse_demo_command(args),
        "graph" => parse_graph_command(args),
        "rhai" => parse_rhai_command(args),
        "docs" => parse_docs_command(args),
        "contracts" => parse_contracts_command(args),
        "artifact" | "artefact" => parse_artifact_command(args),
        "container" => parse_container_command(args),
        "bootstrap" => parse_bootstrap_command(args),
        "uninstall" => parse_uninstall_command(args),
        "release" => parse_release_command(args),
        "doctor" => parse_doctor(args),
        "tasks" => parse_tasks(args),
        "script" => parse_internal_script_command(args),
        "__gateway-run" => Ok(Command::InternalGateway(InternalGatewayArgs)),
        "__container-lease-reaper" => parse_internal_container_lease_reaper_command(args),
        "__host-process-supervise" => parse_internal_host_process_supervise_command(args),
        "__host-process-stop" => parse_internal_host_process_stop_command(args),
        _ if cmd.starts_with('-') => Err(unknown_argument(cmd)),
        _ => parse_task_command(cmd, args),
    }
}

fn parse_deps_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().peekable();
    let subcommand = match args.peek().map(String::as_str) {
        None | Some("--repo" | "--json") => DepsSubcommand::Status { manager: None },
        Some("--help" | "-h") => return Ok(Command::Help(HelpTopic::Deps)),
        Some("status") => {
            args.next();
            let manager = match args.peek().map(String::as_str) {
                Some("cargo" | "bun") => Some(parse_deps_manager(&args.next().unwrap())?),
                _ => None,
            };
            DepsSubcommand::Status { manager }
        }
        Some("link" | "unlink") => {
            let action = args.next().unwrap();
            let manager = args.next().ok_or_else(|| {
                CliParseError::InvalidArguments(format!(
                    "`effigy deps {action}` requires a package manager (`cargo` or `bun`)"
                ))
            })?;
            let manager = parse_deps_manager(&manager)?;
            let library_path = args.next().ok_or_else(|| {
                CliParseError::InvalidArguments(format!(
                    "`effigy deps {action} {}` requires a library path",
                    manager.as_str()
                ))
            })?;
            if library_path.starts_with('-') {
                return Err(CliParseError::InvalidArguments(format!(
                    "`effigy deps {action} {}` requires a library path before flags",
                    manager.as_str()
                )));
            }
            if action == "link" {
                DepsSubcommand::Link {
                    manager,
                    library_path: PathBuf::from(library_path),
                    dry_run: false,
                }
            } else {
                DepsSubcommand::Unlink {
                    manager,
                    library_path: PathBuf::from(library_path),
                    dry_run: false,
                }
            }
        }
        Some(other) => return Err(unknown_argument(other)),
    };

    let mut repo_override = None;
    let mut output_json = false;
    let mut dry_run = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--dry-run" => dry_run = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deps)),
            other => return Err(unknown_argument(other)),
        }
    }

    let subcommand = match subcommand {
        DepsSubcommand::Status { manager: _ } if dry_run => {
            return Err(CliParseError::InvalidArguments(
                "`--dry-run` is accepted only by `effigy deps link` and `effigy deps unlink`"
                    .to_owned(),
            ));
        }
        DepsSubcommand::Status { manager } => DepsSubcommand::Status { manager },
        DepsSubcommand::Link {
            manager,
            library_path,
            ..
        } => DepsSubcommand::Link {
            manager,
            library_path,
            dry_run,
        },
        DepsSubcommand::Unlink {
            manager,
            library_path,
            ..
        } => DepsSubcommand::Unlink {
            manager,
            library_path,
            dry_run,
        },
    };

    Ok(Command::Deps(DepsArgs {
        subcommand,
        repo_override,
        output_json,
    }))
}

fn parse_deps_manager(value: &str) -> Result<DepsManager, CliParseError> {
    match value {
        "cargo" => Ok(DepsManager::Cargo),
        "bun" => Ok(DepsManager::Bun),
        other => Err(CliParseError::InvalidArguments(format!(
            "invalid dependency package manager `{other}` (expected `cargo` or `bun`)"
        ))),
    }
}

fn parse_catalog_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Catalog));
    };

    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Catalog)),
        "cache" => parse_catalog_cache_command(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_catalog_cache_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Catalog));
    };

    let subcommand = match subcommand.as_str() {
        "--help" | "-h" => return Ok(Command::Help(HelpTopic::Catalog)),
        "clear" => CatalogCacheSubcommand::Clear,
        other => return Err(unknown_argument(other)),
    };
    let mut repo_override = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Catalog)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Catalog(CatalogArgs {
        subcommand: CatalogSubcommand::Cache { subcommand },
        repo_override,
        output_json,
    }))
}

fn parse_uninstall_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut plan = false;
    let mut yes = false;
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--plan" => plan = true,
            "--yes" => yes = true,
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Uninstall)),
            other => return Err(unknown_argument(other)),
        }
    }

    if plan && yes {
        return Err(CliParseError::InvalidArguments(
            "`effigy uninstall` does not accept both `--plan` and `--yes`".to_owned(),
        ));
    }

    Ok(Command::Uninstall(UninstallArgs {
        plan,
        yes,
        output_json,
    }))
}

fn parse_rhai_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Rhai));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Rhai)),
        "surface" => parse_rhai_surface_command(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_rhai_surface_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Rhai)),
            other => return Err(unknown_argument(other)),
        }
    }
    Ok(Command::Rhai(RhaiArgs {
        subcommand: RhaiSubcommand::Surface,
        output_json,
    }))
}

fn parse_defer_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut request: Option<String> = None;
    let mut request_args = Vec::new();

    while let Some(arg) = args.next() {
        if request.is_none() {
            match arg.as_str() {
                "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
                "--json" => output_json = true,
                "--help" | "-h" => return Ok(Command::Help(HelpTopic::Defer)),
                other if other.starts_with('-') => return Err(unknown_argument(other)),
                _ => {
                    request = Some(arg);
                    request_args.extend(args);
                    break;
                }
            }
        } else {
            request_args.push(arg);
            request_args.extend(args);
            break;
        }
    }

    let request = request.ok_or(CliParseError::MissingTaskNameValue)?;
    Ok(Command::Defer(DeferArgs {
        task: TaskInvocation {
            name: request,
            args: request_args,
        },
        repo_override,
        output_json,
    }))
}

fn parse_internal_container_lease_reaper_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_root = None;
    let mut container_name = None;
    let mut token = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo-root" => {
                repo_root = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-root".to_owned(),
                    },
                )?));
            }
            "--container" => {
                container_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--container".to_owned(),
                    },
                )?);
            }
            "--token" => {
                token = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--token".to_owned(),
                    },
                )?);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::InternalContainerLeaseReaper(
        InternalContainerLeaseReaperArgs {
            repo_root: repo_root.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--repo-root".to_owned(),
            })?,
            container_name: container_name.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--container".to_owned(),
            })?,
            token: token.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--token".to_owned(),
            })?,
        },
    ))
}

fn parse_internal_host_process_supervise_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_root: Option<PathBuf> = None;
    let mut container_name: Option<String> = None;
    let mut process_name: Option<String> = None;
    let mut run: Option<String> = None;
    let mut pid_file: Option<PathBuf> = None;
    let mut log_file: Option<PathBuf> = None;
    let mut restart: Option<String> = None;
    let mut restart_delay_ms: Option<u64> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo-root" => {
                repo_root = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-root".to_owned(),
                    },
                )?));
            }
            "--container" => {
                container_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--container".to_owned(),
                    },
                )?);
            }
            "--name" => {
                process_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--name".to_owned(),
                    },
                )?);
            }
            "--run" => {
                run = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--run".to_owned(),
                    },
                )?);
            }
            "--pid-file" => {
                pid_file = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--pid-file".to_owned(),
                    },
                )?));
            }
            "--log-file" => {
                log_file = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--log-file".to_owned(),
                    },
                )?));
            }
            "--restart" => {
                restart = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--restart".to_owned(),
                    },
                )?);
            }
            "--restart-delay-ms" => {
                let raw = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--restart-delay-ms".to_owned(),
                    },
                )?;
                restart_delay_ms =
                    Some(
                        raw.parse::<u64>()
                            .map_err(|_| CliParseError::InvalidFlagValue {
                                flag: "--restart-delay-ms".to_owned(),
                                value: raw,
                                expected: "non-negative integer milliseconds".to_owned(),
                            })?,
                    );
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::InternalHostProcessSupervise(
        InternalHostProcessSuperviseArgs {
            repo_root: repo_root.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--repo-root".to_owned(),
            })?,
            container_name: container_name.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--container".to_owned(),
            })?,
            process_name: process_name.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--name".to_owned(),
            })?,
            run: run.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--run".to_owned(),
            })?,
            pid_file: pid_file.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--pid-file".to_owned(),
            })?,
            log_file: log_file.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--log-file".to_owned(),
            })?,
            restart: restart.unwrap_or_else(|| "on-failure".to_owned()),
            restart_delay_ms: restart_delay_ms.unwrap_or(1_000),
        },
    ))
}

fn parse_internal_host_process_stop_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut pid_file: Option<PathBuf> = None;
    let mut signal: Option<String> = None;
    let mut grace_secs: Option<u64> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pid-file" => {
                pid_file = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--pid-file".to_owned(),
                    },
                )?));
            }
            "--signal" => {
                signal = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--signal".to_owned(),
                    },
                )?);
            }
            "--grace-secs" => {
                let raw = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--grace-secs".to_owned(),
                    },
                )?;
                grace_secs =
                    Some(
                        raw.parse::<u64>()
                            .map_err(|_| CliParseError::InvalidFlagValue {
                                flag: "--grace-secs".to_owned(),
                                value: raw,
                                expected: "non-negative integer seconds".to_owned(),
                            })?,
                    );
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::InternalHostProcessStop(
        InternalHostProcessStopArgs {
            pid_file: pid_file.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--pid-file".to_owned(),
            })?,
            signal: signal.unwrap_or_else(|| "SIGTERM".to_owned()),
            grace_secs: grace_secs.unwrap_or(10),
        },
    ))
}

fn parse_internal_script_run_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut file = None;
    let mut repo_root = None;
    let mut task_name = None;
    let mut script_args = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--file".to_owned(),
                    },
                )?;
                file = Some(PathBuf::from(value));
            }
            "--repo-root" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-root".to_owned(),
                    },
                )?;
                repo_root = Some(PathBuf::from(value));
            }
            "--task-name" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--task-name".to_owned(),
                    },
                )?;
                task_name = Some(value);
            }
            "--" => {
                script_args.extend(args);
                break;
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => script_args.push(other.to_owned()),
        }
    }

    let Some(file) = file else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--file".to_owned(),
        });
    };

    Ok(Command::InternalScriptRun(InternalScriptRunArgs {
        file,
        repo_root,
        task_name,
        args: script_args,
    }))
}

fn parse_internal_script_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    match args.next().as_deref() {
        Some("run") => parse_internal_script_run_command(args),
        Some(other) => Err(unknown_argument(other)),
        None => Err(CliParseError::MissingTaskNameValue),
    }
}

fn parse_version_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(arg) = args.next() else {
        return Ok(Command::Version);
    };

    match arg.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::General)),
        other => Err(unknown_argument(other)),
    }
}

fn builtin_help_topic(cmd: &str) -> Option<HelpTopic> {
    crate::help::builtin_help_topic(cmd)
}

fn parse_bundle_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bundle));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Bundle)),
        "inspect" => parse_bundle_inspect(args),
        "sync" => parse_bundle_sync(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_bundle_inspect<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut output_json = false;
    let mut repo_override: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bundle)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Bundle(BundleArgs {
        subcommand: BundleSubcommand::Inspect,
        repo_override,
        output_json,
    }))
}

fn parse_bundle_sync<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bundle)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Bundle(BundleArgs {
        subcommand: BundleSubcommand::Sync,
        repo_override: None,
        output_json,
    }))
}

fn parse_contracts_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Contracts));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Contracts)),
        "validate-selection" => parse_contracts_validate_selection(args),
        "check-json" => parse_contracts_check_json(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_contracts_validate_selection<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut contract_path: Option<PathBuf> = None;
    let mut artifact_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--contract" => {
                contract_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--contract".to_owned(),
                    },
                )?));
            }
            "--artifact" => {
                artifact_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifact".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Contracts)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::ValidateSelection {
            contract_path,
            artifact_path,
        },
        repo_override,
        output_json,
    }))
}

fn parse_contracts_check_json<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut index_path: Option<PathBuf> = None;
    let mut mode = ContractsCheckMode::Full;
    let mut changed_only_base: Option<String> = None;
    let mut print_selected = ContractsSelectionPrintMode::None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--index" => {
                index_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--index".to_owned(),
                    },
                )?));
            }
            "--fast" => mode = ContractsCheckMode::Fast,
            "--full" => mode = ContractsCheckMode::Full,
            "--changed-only" | "--changed-only-base" => {
                changed_only_base = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue { flag: arg },
                )?);
            }
            "--print-selected" | "--print-selected=text" => {
                print_selected = ContractsSelectionPrintMode::Text;
            }
            "--print-selected=json" => {
                print_selected = ContractsSelectionPrintMode::Json;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Contracts)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::CheckJson {
            index_path,
            mode,
            changed_only_base,
            print_selected,
        },
        repo_override,
        output_json,
    }))
}

fn parse_tasks<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut passthrough: Vec<String> = Vec::new();
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut status_selector: Option<String> = None;
    let mut status_mode = false;
    let mut status_all = false;
    let mut output_json = false;
    let mut pretty_json = true;
    let mut pretty_seen = false;

    while let Some(arg) = args.next() {
        if matches!(arg.as_str(), "migrate" | "unlock" | "cache") {
            passthrough.push(arg);
            passthrough.extend(args);
            return Ok(Command::Task(TaskInvocation {
                name: "tasks".to_owned(),
                args: passthrough,
            }));
        }
        match arg.as_str() {
            "status" => {
                if task_name.is_some()
                    || resolve_selector.is_some()
                    || status_selector.is_some()
                    || status_all
                    || status_mode
                    || pretty_seen
                {
                    return Err(CliParseError::InvalidArguments(
                        "`tasks status` cannot be combined with task listing filters or probes"
                            .to_owned(),
                    ));
                }
                status_mode = true;
            }
            "--repo" => {
                passthrough.push(arg);
                let path = parse_repo_path(&mut args)?;
                passthrough.push(path.display().to_string());
                repo_override = Some(path);
            }
            "--task" => {
                if status_selector.is_some() || status_all {
                    return Err(CliParseError::InvalidArguments(
                        "`--task` is not supported together with `tasks status`".to_owned(),
                    ));
                }
                passthrough.push(arg);
                let value = next_required_value(&mut args, CliParseError::MissingTaskNameValue)?;
                passthrough.push(value.clone());
                task_name = Some(value);
            }
            "--resolve" => {
                if status_selector.is_some() || status_all {
                    return Err(CliParseError::InvalidArguments(
                        "`--resolve` is not supported together with `tasks status`".to_owned(),
                    ));
                }
                passthrough.push(arg);
                let value =
                    next_required_value(&mut args, CliParseError::MissingResolveSelectorValue)?;
                passthrough.push(value.clone());
                resolve_selector = Some(value);
            }
            "--json" => {
                passthrough.push(arg);
                output_json = true;
            }
            "--all" if status_mode => {
                if status_selector.is_some() {
                    return Err(CliParseError::InvalidArguments(
                        "`tasks status` accepts either `--all` or one selector, not both"
                            .to_owned(),
                    ));
                }
                status_all = true;
            }
            "--pretty" => {
                if status_selector.is_some() || status_all {
                    return Err(CliParseError::InvalidArguments(
                        "`--pretty` is not supported together with `tasks status`".to_owned(),
                    ));
                }
                let value = next_required_value(&mut args, CliParseError::MissingPrettyValue)?;
                pretty_json = parse_pretty_bool(value.clone())?;
                pretty_seen = true;
                passthrough.push(arg);
                passthrough.push(value);
            }
            other if status_mode && !other.starts_with('-') && status_selector.is_none() => {
                if status_all {
                    return Err(CliParseError::InvalidArguments(
                        "`tasks status` accepts either `--all` or one selector, not both"
                            .to_owned(),
                    ));
                }
                status_selector = Some(other.to_owned());
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(unknown_argument(other)),
        }
    }

    if status_mode && !status_all && status_selector.is_none() {
        return Err(CliParseError::MissingStatusSelectorValue);
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
        resolve_selector,
        status_selector,
        status_all,
        output_json,
        pretty_json,
    }))
}

fn parse_doctor<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;
    let mut explain: Option<TaskInvocation> = None;

    while let Some(arg) = args.next() {
        if let Some(request) = explain.as_mut() {
            request.args.push(arg);
            continue;
        }
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--fix" => fix = true,
            "--verbose" => verbose = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Doctor)),
            other => {
                explain = Some(TaskInvocation {
                    name: other.to_owned(),
                    args: Vec::new(),
                })
            }
        }
    }

    Ok(Command::Doctor(DoctorArgs {
        repo_override,
        output_json,
        fix,
        verbose,
        explain,
    }))
}

fn parse_task_command<I>(name: String, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let task_args = args.into_iter().collect::<Vec<String>>();
    if let Some(topic) = builtin_help_topic(&name) {
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(topic));
        }
    }
    Ok(Command::Task(TaskInvocation {
        name,
        args: task_args,
    }))
}
