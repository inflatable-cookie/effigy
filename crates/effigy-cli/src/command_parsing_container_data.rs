use std::path::PathBuf;

use crate::value_parsing::next_required_value;
use crate::{
    BootstrapDbSeedInput, Command, ContainerArgs, ContainerDataSubcommand, ContainerDbDumpInput,
    ContainerSubcommand, HelpTopic,
};

use super::common::{shared_flag_arm, CommonArgs, CommonFlag};
use super::{unknown_argument, CliParseError};

pub(super) fn parse_container_data<I>(
    name: Option<String>,
    args: I,
) -> Result<Command, CliParseError>
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
    let mut common = CommonArgs::default();

    while let Some(arg) = args.next() {
        shared_flag_arm!(arg.as_str(), &mut args, common, HelpTopic::Container);
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::List,
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
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
    let mut common = CommonArgs::default();
    let mut db_dumps = Vec::new();
    let mut push = false;

    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Container)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            "--push" => push = true,
            "--db-dump" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-dump".to_owned(),
                    },
                )?;
                db_dumps.push(parse_container_db_dump(value)?);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => db_dumps.push(parse_container_db_dump(other.to_owned())?),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Dump { db_dumps, push },
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

fn parse_container_db_dump(value: String) -> Result<ContainerDbDumpInput, CliParseError> {
    if looks_like_bare_db_dump_target(&value) {
        return Ok(ContainerDbDumpInput {
            target: Some(value.clone()),
            path: PathBuf::from(format!("{value}.sql")),
        });
    }

    let parsed = parse_db_seed_input(value)?;
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
    let mut common = CommonArgs::default();
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--yes" => yes = true,
            other => shared_flag_arm!(other, &mut args, common, HelpTopic::Container),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::Import { volume, path, yes },
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
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
    let mut common = CommonArgs::default();
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--yes" => yes = true,
            other => shared_flag_arm!(other, &mut args, common, HelpTopic::Container),
        }
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: ContainerDataSubcommand::PullProduction { yes },
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
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
    let mut common = CommonArgs::default();
    let mut no_prompt = false;
    let mut yes = false;
    let mut db_seeds = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--db-seed" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--db-seed".to_owned(),
                    },
                )?;
                db_seeds.push(parse_db_seed_input(value)?);
            }
            "--no-prompt" => no_prompt = true,
            "--yes" => yes = true,
            other => shared_flag_arm!(other, &mut args, common, HelpTopic::Container),
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
        repo_override: common.repo_override,
        output_json: common.output_json,
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
    let mut common = CommonArgs::default();

    while let Some(arg) = args.next() {
        shared_flag_arm!(arg.as_str(), &mut args, common, HelpTopic::Container);
    }

    Ok(Command::Container(ContainerArgs {
        subcommand: ContainerSubcommand::Data {
            name,
            subcommand: build(volume, path),
        },
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

/// `<file>`, `<target>=<file>` or a bare target name reading `./<target>.sql`.
fn parse_db_seed_input(value: String) -> Result<BootstrapDbSeedInput, CliParseError> {
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

    if looks_like_bare_db_target(&value) {
        return Ok(BootstrapDbSeedInput {
            target: Some(value.clone()),
            path: PathBuf::from(format!("{value}.sql")),
        });
    }

    Ok(BootstrapDbSeedInput {
        target: None,
        path: PathBuf::from(value),
    })
}

fn looks_like_bare_db_target(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('/')
        && !value.contains('\\')
        && !value.starts_with('.')
        && !value.contains('.')
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}
