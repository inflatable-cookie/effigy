use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{
    Command, ExecArgs, GatewayArgs, GatewayPrivateArgs, GatewaySubcommand, HelpTopic, ServiceArgs,
    ServiceSubcommand, WorkspaceArgs,
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
        "setup" => GatewaySubcommand::Setup {
            suffixes: vec![],
            remove: false,
        },
        "up" => GatewaySubcommand::Up,
        "down" => GatewaySubcommand::Down,
        "status" => GatewaySubcommand::Status,
        "repair" => GatewaySubcommand::Repair { yes: false },
        "recover" => return Err(CliParseError::InvalidArguments(
            "`effigy gateway recover` is retired; v0.14.1 gateways use `effigy gateway up`; for a pre-0.14 gateway ask the host administrator to verify and stop it before removing its old PID/version records".to_owned(),
        )),
        "setup-tls" => GatewaySubcommand::SetupTls,
        "setup-listener-handoff" => GatewaySubcommand::SetupListenerHandoff,
        "listener-handoff-status" => GatewaySubcommand::ListenerHandoffStatus,
        "remove-listener-handoff" => GatewaySubcommand::RemoveListenerHandoff,
        other => return Err(unknown_argument(other)),
    };

    let mut private_state_root = None;
    let mut dns_addr = None;
    let mut proxy_addr = None;
    let mut https_addr = None;
    let mut args = args.peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--suffix" if matches!(subcommand, GatewaySubcommand::Setup { .. }) => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--suffix".into(),
                    },
                )?;
                if let GatewaySubcommand::Setup { suffixes, .. } = &mut subcommand {
                    suffixes.push(value);
                }
            }
            "--remove" if matches!(subcommand, GatewaySubcommand::Setup { .. }) => {
                if let GatewaySubcommand::Setup { remove, .. } = &mut subcommand {
                    *remove = true;
                }
            }
            "--json" => output_json = true,
            "--private-state-root"
                if matches!(
                    subcommand,
                    GatewaySubcommand::Up
                        | GatewaySubcommand::Down
                        | GatewaySubcommand::Status
                        | GatewaySubcommand::SetupTls
                ) && private_state_root.is_none() =>
            {
                private_state_root = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--private-state-root".to_owned(),
                    },
                )?));
            }
            "--dns-addr" if matches!(subcommand, GatewaySubcommand::Up) && dns_addr.is_none() => {
                dns_addr = Some(parse_gateway_addr(
                    "--dns-addr",
                    next_required_value(
                        &mut args,
                        CliParseError::MissingFlagValue {
                            flag: "--dns-addr".to_owned(),
                        },
                    )?,
                )?);
            }
            "--proxy-addr"
                if matches!(subcommand, GatewaySubcommand::Up) && proxy_addr.is_none() =>
            {
                proxy_addr = Some(parse_gateway_addr(
                    "--proxy-addr",
                    next_required_value(
                        &mut args,
                        CliParseError::MissingFlagValue {
                            flag: "--proxy-addr".to_owned(),
                        },
                    )?,
                )?);
            }
            "--https-addr"
                if matches!(subcommand, GatewaySubcommand::Up) && https_addr.is_none() =>
            {
                https_addr = Some(parse_gateway_addr(
                    "--https-addr",
                    next_required_value(
                        &mut args,
                        CliParseError::MissingFlagValue {
                            flag: "--https-addr".to_owned(),
                        },
                    )?,
                )?);
            }
            "--yes" if matches!(subcommand, GatewaySubcommand::Repair { .. }) => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Gateway)),
            other => return Err(unknown_argument(other)),
        }
    }

    if matches!(subcommand, GatewaySubcommand::Repair { .. }) {
        subcommand = GatewaySubcommand::Repair { yes };
    }

    if private_state_root.is_none()
        && (dns_addr.is_some() || proxy_addr.is_some() || https_addr.is_some())
    {
        return Err(CliParseError::InvalidArguments(
            "gateway bind overrides require `--private-state-root`".to_owned(),
        ));
    }

    Ok(Command::Gateway(GatewayArgs {
        subcommand,
        private: private_state_root.map(|state_root| GatewayPrivateArgs {
            state_root,
            dns_addr,
            proxy_addr,
            https_addr,
        }),
        output_json,
    }))
}

pub(super) fn parse_gateway_addr(
    flag: &str,
    value: String,
) -> Result<std::net::SocketAddr, CliParseError> {
    value.parse().map_err(|error| {
        CliParseError::InvalidArguments(format!(
            "{flag} expects an IP socket address, got {value:?}: {error}"
        ))
    })
}

const RETIRED_SYSTEM_SUFFIX: &str =
    "See CHANGELOG.md `[Unreleased]` > Removed for the v0.15 surface cut";

/// `effigy system` retired in v0.15: every old spelling names its exact
/// `effigy container` replacement before the argument is rejected.
pub(super) fn parse_system_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    const GENERAL_REPLACEMENT: &str = "Use `effigy container up|down|status|logs` for environment lifecycle and `effigy container profile repair|reset-runtime` for runtime recovery";
    let mut args = args.into_iter();
    let first = args.next();
    let replacement = match first.as_deref() {
        Some("up") => "Run `effigy container up` to start the environment",
        Some("down") => "Run `effigy container down` to stop the environment",
        Some("status") => "Run `effigy container status` to read environment state",
        Some("logs" | "--follow") => "Run `effigy container logs` to read environment logs",
        Some("repair") => "Run `effigy container profile repair` to recover the Colima runtime",
        Some("reset-runtime") => {
            "Run `effigy container profile reset-runtime` to stop the Colima runtime without deleting it"
        }
        Some("--help" | "-h") => {
            "Use `effigy help container` for environment lifecycle and recovery help"
        }
        // No subcommand, or one this release never had: still name the
        // lifecycle family rather than fall through as an unknown argument.
        Some(_) | None => GENERAL_REPLACEMENT,
    };
    let spelling = match first.as_deref() {
        Some(word) if !word.starts_with('-') => format!("`effigy system {word}`"),
        _ => "`effigy system`".to_owned(),
    };
    Err(CliParseError::InvalidArguments(format!(
        "{spelling} is retired in v0.15. {replacement}. {RETIRED_SYSTEM_SUFFIX}"
    )))
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

const SERVICE_PACK_RETIRED_ERROR: &str = "`effigy service pack` is retired in v0.15 with the catalog-pack runtime. Services resolve from the project catalog (`infra/dev/catalog/`), the bundle's `catalog/`, declared `[catalog.sources.<name>]` sources pinned by `effigy bundle sync`, and the compiled baseline; `effigy service list` shows where each service comes from. Installed pack state is ignored and left untouched";

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
        "pack" => Err(CliParseError::InvalidArguments(
            SERVICE_PACK_RETIRED_ERROR.to_owned(),
        )),
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
