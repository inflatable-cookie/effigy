use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{
    Command, ExecArgs, GatewayArgs, GatewaySubcommand, HelpTopic, ServiceArgs,
    ServicePackInstallSource, ServicePackSubcommand, ServiceSubcommand, SystemArgs,
    SystemSubcommand, WorkspaceArgs,
};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_gateway_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Gateway));
    };

    let mut output_json = false;
    let mut yes = false;
    let mut subcommand = match subcmd.as_str() {
        "--help" | "-h" => return Ok(Command::Help(HelpTopic::Gateway)),
        "up" => GatewaySubcommand::Up,
        "down" => GatewaySubcommand::Down,
        "status" => GatewaySubcommand::Status,
        "repair" => GatewaySubcommand::Repair { yes: false },
        "setup-tls" => GatewaySubcommand::SetupTls,
        other => return Err(unknown_argument(other)),
    };

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--yes" if matches!(subcommand, GatewaySubcommand::Repair { .. }) => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Gateway)),
            other => return Err(unknown_argument(other)),
        }
    }

    if matches!(subcommand, GatewaySubcommand::Repair { .. }) {
        subcommand = GatewaySubcommand::Repair { yes };
    }

    Ok(Command::Gateway(GatewayArgs {
        subcommand,
        output_json,
    }))
}

pub(super) fn parse_system_command<I>(args: I) -> Result<Command, CliParseError>
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

pub(super) fn parse_workspace_command<I>(args: I) -> Result<Command, CliParseError>
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

pub(super) fn parse_exec_command<I>(args: I) -> Result<Command, CliParseError>
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

pub(super) fn parse_service_command<I>(args: I) -> Result<Command, CliParseError>
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
        "pack" => parse_service_pack(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_service_pack<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Service));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Service)),
        "status" => parse_service_pack_simple(args, ServicePackSubcommand::Status),
        "rollback" => parse_service_pack_simple(args, ServicePackSubcommand::Rollback),
        "reset" => parse_service_pack_simple(args, ServicePackSubcommand::Reset),
        "install" => parse_service_pack_install(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_service_pack_simple<I>(
    args: I,
    subcommand: ServicePackSubcommand,
) -> Result<Command, CliParseError>
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
        subcommand: ServiceSubcommand::Pack(subcommand),
        repo_override,
        output_json,
    }))
}

fn parse_service_pack_install<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut path: Option<PathBuf> = None;
    let mut reference: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--path" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    },
                )?;
                path = Some(PathBuf::from(value));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Service)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other if reference.is_none() => reference = Some(other.to_owned()),
            other => return Err(unknown_argument(other)),
        }
    }

    // Exactly one candidate. Ambiguity here would decide what gets activated,
    // so it fails rather than picking a winner.
    let source = match (reference, path) {
        (Some(_), Some(_)) => {
            return Err(CliParseError::UnknownArgument(
                "--path with an `oci://` reference".to_owned(),
            ))
        }
        (Some(reference), None) => ServicePackInstallSource::Oci { reference },
        (None, Some(path)) => ServicePackInstallSource::Path { path },
        (None, None) => {
            return Err(CliParseError::MissingFlagValue {
                flag: "oci://<REPO>@sha256:<DIGEST> or --path <DIR>".to_owned(),
            })
        }
    };

    Ok(Command::Service(ServiceArgs {
        subcommand: ServiceSubcommand::Pack(ServicePackSubcommand::Install { source }),
        repo_override,
        output_json,
    }))
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
