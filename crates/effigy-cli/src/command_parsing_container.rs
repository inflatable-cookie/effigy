use std::path::PathBuf;

use crate::{Command, ContainerArgs, ContainerCacheSubcommand, ContainerSubcommand, HelpTopic};

use crate::value_parsing::{next_required_value, parse_repo_path};

use super::container_data::parse_container_data;
use super::{unknown_argument, CliParseError};

pub(super) fn parse_container_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    const ACTIONS: [&str; 10] = [
        "up", "down", "status", "stats", "logs", "shell", "reset", "cache", "data", "eject",
    ];

    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Ok(Command::Help(HelpTopic::Container));
    };

    if matches!(first.as_str(), "--help" | "-h") {
        return Ok(Command::Help(HelpTopic::Container));
    }

    let (name, action) = if ACTIONS.contains(&first.as_str()) {
        (None, first)
    } else {
        let Some(action) = args.next() else {
            return Err(unknown_argument(first));
        };
        (Some(first), action)
    };

    match action.as_str() {
        "up" => parse_container_up(name, args),
        "down" => parse_container_down(name, args),
        "status" => parse_container_status(name, args),
        "stats" => parse_container_stats(name, args),
        "logs" => parse_container_logs(name, args),
        "shell" => parse_container_shell(name, args),
        "reset" => parse_container_reset(name, args),
        "cache" => parse_container_cache(name, args),
        "data" => parse_container_data(name, args),
        "eject" => parse_container_eject(name, args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_up<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut attach = false;
    let mut detach = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--attach" => attach = true,
            "--detach" => detach = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Up {
            name,
            attach,
            detach,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_down<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--all" => all = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if all && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> down` does not accept `--all`; use `effigy container down --all` for cross-project shutdown".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Down { name, all },
        repo_override,
        output_json,
    }))
}

fn parse_container_status<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--all" => all = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if all && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> status` does not accept `--all`; use `effigy container status --all` for cross-project discovery".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Status { name, all },
        repo_override,
        output_json,
    }))
}

fn parse_container_reset<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut keep_data = false;
    let mut wipe_data = false;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--keep-data" => keep_data = true,
            "--wipe-data" => wipe_data = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }
    if keep_data && wipe_data {
        return Err(CliParseError::InvalidArguments(
            "`effigy container reset` does not accept both `--keep-data` and `--wipe-data`"
                .to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Reset {
            name,
            keep_data,
            wipe_data,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_stats<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut all = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--all" => all = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> stats` is not supported yet; use `effigy container stats --all` for the bounded cross-project resource view".to_owned(),
        ));
    }
    if !all {
        return Err(CliParseError::InvalidArguments(
            "`effigy container stats` currently requires `--all`; use `effigy container stats --all` for cross-project resource discovery".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Stats { all },
        repo_override,
        output_json,
    }))
}

fn parse_container_eject<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    parse_named_container_simple_subcommand(name, args, |name| ContainerSubcommand::Eject { name })
}

fn parse_container_cache<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`effigy container cache` requires a subcommand; use `list` or `prune`".to_owned(),
        ));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Container)),
        "list" => parse_container_cache_list(name, args),
        "prune" => parse_container_cache_prune(name, args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_cache_list<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut all = false;
    let mut project: Option<String> = None;
    let mut kind: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--all" => all = true,
            "--project" => {
                project = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--project".to_owned(),
                    },
                )?)
            }
            "--kind" => {
                kind = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--kind".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if project.is_some() || kind.is_some() {
        all = true;
    }

    if all && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> cache list` does not accept `--all`; use `effigy container cache list --all` for cross-project cache discovery".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Cache {
            name,
            subcommand: ContainerCacheSubcommand::List { all, project, kind },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_cache_prune<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut all = false;
    let mut yes = false;
    let mut project: Option<String> = None;
    let mut kind: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--all" => all = true,
            "--yes" => yes = true,
            "--project" => {
                project = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--project".to_owned(),
                    },
                )?)
            }
            "--kind" => {
                kind = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--kind".to_owned(),
                    },
                )?)
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    if project.is_some() || kind.is_some() {
        all = true;
    }

    if all && name.is_some() {
        return Err(CliParseError::InvalidArguments(
            "`effigy container <NAME> cache prune` does not accept `--all`; use `effigy container cache prune --all` for profile-wide cache cleanup".to_owned(),
        ));
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Cache {
            name,
            subcommand: ContainerCacheSubcommand::Prune {
                all,
                yes,
                project,
                kind,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_logs<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut service: Option<String> = None;
    let mut follow = false;

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
            "--follow" => follow = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Logs {
            name,
            service,
            follow,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_shell<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut service: Option<String> = None;
    let mut command: Option<String> = None;

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
            "--command" => {
                command = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--command".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Shell {
            name,
            service,
            command,
        },
        repo_override,
        output_json,
    }))
}

fn parse_named_container_simple_subcommand<I, F>(
    name: Option<String>,
    args: I,
    build: F,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
    F: FnOnce(Option<String>) -> ContainerSubcommand,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: build(name),
        repo_override,
        output_json,
    }))
}
