use std::path::PathBuf;

use crate::{Command, DeployArgs, DeploySubcommand, HelpTopic};

use crate::value_parsing::{next_required_value, parse_repo_path};

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
        "plan" => parse_deploy_plan(args),
        "apply" => parse_deploy_apply(args),
        "status" => parse_deploy_status(args),
        "history" => parse_deploy_history(args),
        "redeploy" => parse_deploy_redeploy(args),
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

    if provider.starts_with('-') || provider.trim().is_empty() {
        return Err(unknown_argument(&provider));
    }

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

fn parse_deploy_plan<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut write_report = false;
    let mut env: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--write-report" => write_report = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if env.is_none() => env = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected deploy plan argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Plan {
            env: env.ok_or_else(|| {
                CliParseError::InvalidArguments("`deploy plan` requires `<ENV>`".to_owned())
            })?,
            write_report,
        },
        repo_override,
        output_json,
    }))
}

fn parse_deploy_apply<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut yes = false;
    let mut env: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if env.is_none() => env = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected deploy apply argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Apply {
            env: env.ok_or_else(|| {
                CliParseError::InvalidArguments("`deploy apply` requires `<ENV>`".to_owned())
            })?,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_deploy_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let (env, repo_override, output_json) = parse_env_read_command(args, "status")?;
    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Status { env },
        repo_override,
        output_json,
    }))
}

fn parse_deploy_history<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut env: Option<String> = None;
    let mut limit: Option<usize> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => {
                let raw = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--limit".to_owned(),
                    },
                )?;
                let parsed = raw.parse::<usize>().map_err(|_| {
                    CliParseError::InvalidArguments(format!(
                        "`--limit` must be a positive integer, got `{raw}`"
                    ))
                })?;
                if parsed == 0 {
                    return Err(CliParseError::InvalidArguments(
                        "`--limit` must be a positive integer, got `0`".to_owned(),
                    ));
                }
                limit = Some(parsed);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if env.is_none() => env = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected deploy history argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::History {
            env: env.ok_or_else(|| {
                CliParseError::InvalidArguments("`deploy history` requires `<ENV>`".to_owned())
            })?,
            limit,
        },
        repo_override,
        output_json,
    }))
}

fn parse_deploy_redeploy<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut yes = false;
    let mut env: Option<String> = None;
    let mut deployment: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--deployment" => {
                deployment = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--deployment".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Deploy)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if env.is_none() => env = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected deploy redeploy argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::Deploy(DeployArgs {
        subcommand: DeploySubcommand::Redeploy {
            env: env.ok_or_else(|| {
                CliParseError::InvalidArguments("`deploy redeploy` requires `<ENV>`".to_owned())
            })?,
            deployment: deployment.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--deployment".to_owned(),
            })?,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_env_read_command<I>(
    args: I,
    name: &str,
) -> Result<(String, Option<PathBuf>, bool), CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut env: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => {
                return Err(CliParseError::InvalidArguments(format!(
                    "`deploy {name} --help` is handled by `effigy deploy --help`"
                )))
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if env.is_none() => env = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected deploy {name} argument `{other}`"
                )));
            }
        }
    }

    Ok((
        env.ok_or_else(|| {
            CliParseError::InvalidArguments(format!("`deploy {name}` requires `<ENV>`"))
        })?,
        repo_override,
        output_json,
    ))
}
