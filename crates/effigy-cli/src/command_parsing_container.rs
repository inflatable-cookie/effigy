use std::path::PathBuf;

use crate::{
    Command, ContainerArgs, ContainerDataSubcommand, ContainerDbDumpInput, ContainerSubcommand,
    HelpTopic,
};

use crate::value_parsing::{next_required_value, parse_repo_path};

use super::parse_bootstrap_db_seed;
use super::{unknown_argument, CliParseError};

pub(super) fn parse_container_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    const ACTIONS: [&str; 9] = [
        "up", "down", "status", "stats", "logs", "shell", "reset", "data", "eject",
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

fn parse_container_data<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`effigy container data` requires a subcommand; use `list`, `export`, `dump`, `import`, `pull-production`, or `seed`".to_owned(),
        ));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Container)),
        "list" => parse_container_data_list(name, args),
        "export" => parse_container_data_export(name, args),
        "dump" => parse_container_data_dump(name, args),
        "import" => parse_container_data_import(name, args),
        "pull-production" => parse_container_data_pull_production(name, args),
        "seed" => parse_container_data_seed(name, args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_container_data_list<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::List,
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_data_export<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    parse_container_data_transfer(name, args, "export", |volume, path| {
        ContainerDataSubcommand::Export { volume, path }
    })
}

fn parse_container_data_dump<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut db_dumps = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--db-dump" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-dump".to_owned(),
                    },
                )?;
                db_dumps.push(parse_container_db_dump(value)?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Dump { db_dumps },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_db_dump(value: String) -> Result<ContainerDbDumpInput, CliParseError> {
    if looks_like_bare_db_dump_target(&value) {
        return Ok(ContainerDbDumpInput {
            target: Some(value.clone()),
            path: PathBuf::from(format!("{value}.sql")),
        });
    }

    let parsed = parse_bootstrap_db_seed(value)?;
    Ok(ContainerDbDumpInput {
        target: parsed.target,
        path: parsed.path,
    })
}

fn looks_like_bare_db_dump_target(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('=')
        && !value.contains('/')
        && !value.contains('\\')
        && !value.starts_with('.')
        && !value.contains('.')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

fn parse_container_data_import<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let volume = args.next().ok_or_else(|| {
        CliParseError::InvalidArguments(
            "`effigy container data import` requires <VOLUME> and <PATH>".to_owned(),
        )
    })?;
    let path = PathBuf::from(args.next().ok_or_else(|| {
        CliParseError::InvalidArguments(
            "`effigy container data import` requires <VOLUME> and <PATH>".to_owned(),
        )
    })?);
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Import { volume, path, yes },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_data_pull_production<I>(
    name: Option<String>,
    args: I,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::PullProduction { yes },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_data_seed<I>(name: Option<String>, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    if let Some(container_name) = name.as_deref() {
        return Err(CliParseError::InvalidArguments(format!(
            "`effigy container {container_name} data seed` is not supported; `data seed` currently targets the repo default container only"
        )));
    }

    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut no_prompt = false;
    let mut yes = false;
    let mut db_seeds = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--db-seed" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-seed".to_owned(),
                    },
                )?;
                db_seeds.push(parse_bootstrap_db_seed(value)?);
            }
            "--no-prompt" => no_prompt = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Container)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Seed {
                db_seeds,
                no_prompt,
                yes,
            },
        },
        repo_override,
        output_json,
    }))
}

fn parse_container_data_transfer<I>(
    name: Option<String>,
    args: I,
    action: &str,
    build: impl FnOnce(String, PathBuf) -> ContainerDataSubcommand,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let volume = args.next().ok_or_else(|| {
        CliParseError::InvalidArguments(format!(
            "`effigy container data {action}` requires <VOLUME> and <PATH>"
        ))
    })?;
    let path = PathBuf::from(args.next().ok_or_else(|| {
        CliParseError::InvalidArguments(format!(
            "`effigy container data {action}` requires <VOLUME> and <PATH>"
        ))
    })?);
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
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: build(volume, path),
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
