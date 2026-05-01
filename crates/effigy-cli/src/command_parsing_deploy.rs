use std::path::PathBuf;

use crate::{Command, DeployArgs, DeployExportProvider, DeploySubcommand, HelpTopic};

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
        "export" => parse_deploy_export(args),
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

fn parse_deploy_export<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(provider) = args.next() else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<PROVIDER>".to_owned(),
        });
    };

    let provider = match provider.as_str() {
        "render" => DeployExportProvider::Render,
        "railway" => DeployExportProvider::Railway,
        other => return Err(unknown_argument(other)),
    };

    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut path: Option<PathBuf> = None;
    let mut plan = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--path" => {
                let Some(value) = args.next() else {
                    return Err(CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    });
                };
                path = Some(PathBuf::from(value));
            }
            "--plan" => plan = true,
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other => return Err(unknown_argument(other)),
        }
    }

    let Some(path) = path else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--path".to_owned(),
        });
    };

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Export {
            provider,
            path,
            plan,
        },
        repo_override,
        output_json,
    }))
}
