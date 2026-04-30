use std::path::PathBuf;

use crate::{Command, DeployArgs, DeploySubcommand, HelpTopic};

use crate::value_parsing::parse_repo_path;

use super::{unknown_argument, CliParseError};

pub(super) fn parse_deploy_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Deploy));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Deploy)),
        "model" => parse_deploy_model(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_deploy_model<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Model,
        repo_override,
        output_json,
    }))
}
