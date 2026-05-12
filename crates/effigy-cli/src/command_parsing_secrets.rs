use std::path::PathBuf;

use crate::{Command, HelpTopic, SecretsArgs, SecretsSubcommand};

use crate::value_parsing::parse_repo_path;

use super::{unknown_argument, CliParseError};

pub(super) fn parse_secrets_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Secrets));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Secrets)),
        "list" => parse_secrets_read_command(args, SecretsSubcommand::List),
        "doctor" => parse_secrets_read_command(args, SecretsSubcommand::Doctor),
        "init" => parse_secrets_read_command(args, SecretsSubcommand::Init),
        "set" => parse_secrets_named_command(args, |name| SecretsSubcommand::Set { name }),
        "unset" => parse_secrets_named_command(args, |name| SecretsSubcommand::Unset { name }),
        "unlock" => parse_secrets_read_command(args, SecretsSubcommand::Unlock),
        "lock" => parse_secrets_read_command(args, SecretsSubcommand::Lock),
        other => Err(unknown_argument(other)),
    }
}

fn parse_secrets_read_command<I>(
    args: I,
    subcommand: SecretsSubcommand,
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Secrets)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Secrets(SecretsArgs {
        subcommand,
        repo_override,
        output_json,
    }))
}

fn parse_secrets_named_command<I, F>(args: I, build: F) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
    F: FnOnce(String) -> SecretsSubcommand,
{
    let mut args = args.into_iter();
    let Some(name) = args.next() else {
        return Err(unknown_argument("missing secret name"));
    };
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Secrets)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Secrets(SecretsArgs {
        subcommand: build(name),
        repo_override,
        output_json,
    }))
}
