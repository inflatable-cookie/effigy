use std::path::PathBuf;

#[path = "command_parsing_container.rs"]
mod container;
#[path = "command_parsing_demo.rs"]
mod demo;
#[path = "command_parsing_deploy.rs"]
mod deploy;
#[path = "command_parsing_distribution.rs"]
mod distribution;
#[path = "command_parsing_docs.rs"]
mod docs;

use crate::{
    BootstrapArgs, BootstrapDbSeedInput, BootstrapDepsSyncMode, BootstrapSubcommand, BundleArgs,
    BundleSubcommand, ChangelogArgs, ChangelogSubcommand, Command, ContractsArgs,
    ContractsCheckMode, ContractsSelectionPrintMode, ContractsSubcommand, DeferArgs, DoctorArgs,
    ExecArgs, GatewayArgs, GatewaySubcommand, HelpTopic, InternalContainerLeaseReaperArgs,
    InternalGatewayArgs, InternalHostProcessStopArgs, InternalHostProcessSuperviseArgs,
    InternalRhaiArgs, ReleaseArgs, ReleaseSubcommand, ServiceArgs, ServiceSubcommand, SystemArgs,
    SystemSubcommand, TaskInvocation, TasksArgs, WorkspaceArgs,
};
use container::parse_container_command;
use demo::parse_demo_command;
use deploy::parse_deploy_command;
use distribution::parse_distribution_command;
use docs::parse_docs_command;

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
        "changelog" => parse_changelog_command(args),
        "deploy" => parse_deploy_command(args),
        "defer" => parse_defer_command(args),
        "exec" => parse_exec_command(args),
        "system" => parse_system_command(args),
        "workspace" => parse_workspace_command(args),
        "gateway" => parse_gateway_command(args),
        "service" => parse_service_command(args),
        "demo" => parse_demo_command(args),
        "docs" => parse_docs_command(args),
        "contracts" => parse_contracts_command(args),
        "distribution" => parse_distribution_command(args),
        "container" => parse_container_command(args),
        "bootstrap" => parse_bootstrap(args),
        "release" => parse_release(args),
        "doctor" => parse_doctor(args),
        "tasks" | "catalogs" => parse_tasks(args),
        "__rhai-step" => parse_internal_rhai_command(args),
        "__gateway-run" => Ok(Command::InternalGateway(InternalGatewayArgs)),
        "__container-lease-reaper" => parse_internal_container_lease_reaper_command(args),
        "__host-process-supervise" => parse_internal_host_process_supervise_command(args),
        "__host-process-stop" => parse_internal_host_process_stop_command(args),
        _ if cmd.starts_with('-') => Err(unknown_argument(cmd)),
        _ => parse_task_command(cmd, args),
    }
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

fn parse_internal_rhai_command<I>(args: I) -> Result<Command, CliParseError>
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

    Ok(Command::InternalRhai(InternalRhaiArgs {
        file,
        repo_root,
        task_name,
        args: script_args,
    }))
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
    match cmd {
        "test" => Some(HelpTopic::Test),
        "watch" => Some(HelpTopic::Watch),
        "init" => Some(HelpTopic::Init),
        "migrate" => Some(HelpTopic::Migrate),
        "defer" => Some(HelpTopic::Defer),
        "exec" => Some(HelpTopic::Exec),
        "bundle" => Some(HelpTopic::Bundle),
        "deploy" => Some(HelpTopic::Deploy),
        "system" => Some(HelpTopic::System),
        "workspace" => Some(HelpTopic::Workspace),
        "gateway" => Some(HelpTopic::Gateway),
        "demo" => Some(HelpTopic::Demo),
        "service" => Some(HelpTopic::Service),
        "docs" => Some(HelpTopic::Docs),
        "contracts" => Some(HelpTopic::Contracts),
        "distribution" => Some(HelpTopic::Distribution),
        "container" => Some(HelpTopic::Container),
        "bootstrap" => Some(HelpTopic::Bootstrap),
        "release" => Some(HelpTopic::Release),
        _ => None,
    }
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
        "list" => parse_bundle_list(args),
        "inspect" => parse_bundle_inspect(args),
        "export" => parse_bundle_export(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_bundle_list<I>(args: I) -> Result<Command, CliParseError>
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
        subcommand: BundleSubcommand::List,
        output_json,
    }))
}

fn parse_bundle_inspect<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(bundle) = args.next() else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<BUNDLE>".to_owned(),
        });
    };

    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bundle)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Bundle(BundleArgs {
        subcommand: BundleSubcommand::Inspect { bundle },
        output_json,
    }))
}

fn parse_bundle_export<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(bundle) = args.next() else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<BUNDLE>".to_owned(),
        });
    };

    let mut output_json = false;
    let mut path = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--path" => {
                let Some(value) = args.next() else {
                    return Err(CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    });
                };
                path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bundle)),
            other => return Err(unknown_argument(other)),
        }
    }

    let Some(path) = path else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--path".to_owned(),
        });
    };

    Ok(Command::Bundle(BundleArgs {
        subcommand: BundleSubcommand::Export { bundle, path },
        output_json,
    }))
}

fn parse_gateway_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Gateway));
    };

    let mut output_json = false;
    let subcommand = match subcmd.as_str() {
        "--help" | "-h" => return Ok(Command::Help(HelpTopic::Gateway)),
        "up" => GatewaySubcommand::Up,
        "down" => GatewaySubcommand::Down,
        "status" => GatewaySubcommand::Status,
        "setup-tls" => GatewaySubcommand::SetupTls,
        other => return Err(unknown_argument(other)),
    };

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Gateway)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Gateway(GatewayArgs {
        subcommand,
        output_json,
    }))
}

fn parse_system_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::System));
    };

    let mut subcommand = match subcmd.as_str() {
        "--help" | "-h" => return Ok(Command::Help(HelpTopic::System)),
        "up" => SystemSubcommand::Up,
        "down" => SystemSubcommand::Down,
        "status" => SystemSubcommand::Status,
        "logs" => SystemSubcommand::Logs { follow: false },
        "repair" => SystemSubcommand::Repair,
        "reset-runtime" => SystemSubcommand::ResetRuntime,
        other => return Err(unknown_argument(other)),
    };
    let mut system = None;
    let mut repo_override = None;
    let mut output_json = false;
    let mut follow = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--system" => {
                system = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--system".to_owned(),
                    },
                )?)
            }
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--follow" if matches!(subcommand, SystemSubcommand::Logs { .. }) => follow = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::System)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }

    if matches!(subcommand, SystemSubcommand::Logs { .. }) {
        subcommand = SystemSubcommand::Logs { follow };
    }

    Ok(Command::System(SystemArgs {
        subcommand,
        system,
        repo_override,
        output_json,
    }))
}

fn parse_workspace_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut workspace = None;
    let mut system = None;
    let mut repo_override = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Workspace)),
            "--system" => {
                system = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--system".to_owned(),
                    },
                )?)
            }
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if workspace.is_none() => workspace = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Workspace(WorkspaceArgs {
        workspace,
        system,
        repo_override,
        output_json,
    }))
}

fn parse_exec_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut service: Option<String> = None;
    let mut command = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--service" => {
                service = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--service".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Exec)),
            "--" => {
                command.extend(args);
                break;
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                command.push(arg);
                command.extend(args);
                break;
            }
        }
    }

    if command.is_empty() {
        return Ok(Command::Help(HelpTopic::Exec));
    }

    Ok(Command::Exec(ExecArgs {
        repo_override,
        output_json,
        service,
        command,
    }))
}

fn parse_service_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Service));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Service)),
        "list" => parse_service_list(args),
        "extract" => parse_service_extract(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_service_list<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Service)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Service(ServiceArgs {
        subcommand: ServiceSubcommand::List,
        repo_override,
        output_json,
    }))
}

fn parse_service_extract<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(service) = args.next() else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<SERVICE>".to_owned(),
        });
    };

    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--dir" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--dir".to_owned(),
                    },
                )?;
                dir = Some(PathBuf::from(value));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Service)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Service(ServiceArgs {
        subcommand: ServiceSubcommand::Extract { service, dir },
        repo_override,
        output_json,
    }))
}

fn parse_bootstrap<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };
    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    }
    if first == "deps" {
        return parse_bootstrap_deps(args);
    }
    if first == "teardown" {
        let mut yes = false;
        let mut output_json = false;
        for arg in args.by_ref() {
            match arg.as_str() {
                "--json" => output_json = true,
                "--yes" => yes = true,
                other if other.starts_with('-') => return Err(unknown_argument(other)),
                _ => return Err(unknown_argument(arg)),
            }
        }
        return Ok(Command::Bootstrap(BootstrapArgs {
            subcommand: BootstrapSubcommand::Teardown { yes },
            output_json,
        }));
    }

    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut db_seeds = Vec::<BootstrapDbSeedInput>::new();
    let mut fresh = false;
    let mut no_prompt = false;
    let mut reuse_path = false;
    let mut start = true;
    let mut plan = false;
    let mut output_json = false;
    let repo_url = first;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--fresh" => fresh = true,
            "--no-prompt" => no_prompt = true,
            "--reuse-path" => reuse_path = true,
            "--start" => start = true,
            "--no-start" => start = false,
            "--plan" => plan = true,
            "--path" => {
                path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    },
                )?));
            }
            "--branch" => {
                branch = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--branch".to_owned(),
                    },
                )?);
            }
            "--db-seed" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-seed".to_owned(),
                    },
                )?;
                db_seeds.push(parse_bootstrap_db_seed(value)?);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::Clone {
            repo_url,
            path,
            branch,
            db_seeds,
            fresh,
            no_prompt,
            reuse_path,
            start,
            plan,
        },
        output_json,
    }))
}

pub(super) fn parse_bootstrap_db_seed(
    value: String,
) -> Result<BootstrapDbSeedInput, CliParseError> {
    if let Some((target, path)) = value.split_once('=') {
        if target.is_empty() || path.is_empty() {
            return Err(CliParseError::InvalidArguments(
                "--db-seed requires <file> or <target>=<file>".to_owned(),
            ));
        }
        return Ok(BootstrapDbSeedInput {
            target: Some(target.to_owned()),
            path: PathBuf::from(path),
        });
    }

    Ok(BootstrapDbSeedInput {
        target: None,
        path: PathBuf::from(value),
    })
}

fn parse_bootstrap_deps<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };
    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Bootstrap)),
        "sync" => parse_bootstrap_deps_sync(args),
        other => Err(CliParseError::UnknownArgument(other.to_owned())),
    }
}

fn parse_bootstrap_deps_sync<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter();
    let mut output_json = false;
    let mut mode = BootstrapDepsSyncMode::Both;
    let mut paths = Vec::<String>::new();

    for arg in args {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bootstrap)),
            "--json" => output_json = true,
            "--js-only" => {
                if mode == BootstrapDepsSyncMode::RustOnly {
                    return Err(CliParseError::UnknownArgument(arg.to_owned()));
                }
                mode = BootstrapDepsSyncMode::JsOnly;
            }
            "--rust-only" => {
                if mode == BootstrapDepsSyncMode::JsOnly {
                    return Err(CliParseError::UnknownArgument(arg.to_owned()));
                }
                mode = BootstrapDepsSyncMode::RustOnly;
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(arg),
        }
    }

    if paths.is_empty() {
        paths.push(".".to_owned());
    }

    Ok(Command::Bootstrap(BootstrapArgs {
        subcommand: BootstrapSubcommand::DepsSync { mode, paths },
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
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--task" => {
                task_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingTaskNameValue,
                )?);
            }
            "--resolve" => {
                resolve_selector = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingResolveSelectorValue,
                )?);
            }
            "--json" => {
                output_json = true;
            }
            "--pretty" => {
                let value = next_required_value(&mut args, CliParseError::MissingPrettyValue)?;
                pretty_json = parse_pretty_bool(value)?;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
        resolve_selector,
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

fn parse_release<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Release));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Release)),
        "status" => parse_release_status(args),
        "gates" => parse_release_gates(args),
        "resume" => parse_release_resume(args),
        "verify-install" => parse_release_verify_install(args),
        "simulate" => parse_release_simulate(args),
        "prepare" => parse_release_prepare(args),
        "execute" => parse_release_execute(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_release_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates },
        repo_override,
        output_json,
    }))
}

fn parse_release_gates<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Gates,
        repo_override,
        output_json,
    }))
}

fn parse_release_resume<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Resume { allow_stale },
        repo_override,
        output_json,
    }))
}

fn parse_release_simulate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--version" => {
                version_override = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Simulate { version_override },
        repo_override,
        output_json,
    }))
}

fn parse_release_verify_install<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut repo_url: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::VerifyInstall { tag, repo_url },
        repo_override,
        output_json,
    }))
}

fn parse_release_prepare<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;
    let mut plan = false;
    let mut yes = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
            "--version" => {
                version_override = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Prepare {
            plan,
            check_gates,
            yes,
            version_override,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_execute<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut plan = false;
    let mut yes = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Execute {
            plan,
            yes,
            allow_stale,
        },
        repo_override,
        output_json,
    }))
}

fn parse_changelog_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Changelog));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Changelog)),
        "validate" => parse_changelog_validate(args),
        "format" => parse_changelog_format(args),
        "analyze" => parse_changelog_analyze(args),
        "extract" => parse_changelog_extract(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_changelog_validate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Validate,
        file,
        output_json,
    }))
}

fn parse_changelog_format<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;
    let mut write = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--write" => write = true,
            "--preview" => write = false,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Format { write },
        file,
        output_json,
    }))
}

fn parse_changelog_analyze<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Analyze,
        file,
        output_json,
    }))
}

fn parse_changelog_extract<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;
    let mut version: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--version" => {
                version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    let version = version.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "--version".to_owned(),
    })?;

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Extract { version },
        file,
        output_json,
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
