use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{unknown_argument, CliParseError, Command, HelpTopic, StateArgs, StateSubcommand};

pub(super) fn parse_state_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::State));
    };

    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::State)),
        "plan" => parse_state_plan_command(args),
        "apply" => parse_state_apply_command(args),
        "capture" => parse_state_capture_command(args),
        "history" => parse_state_history_command(args),
        other if other.starts_with('-') => Err(unknown_argument(other)),
        other => Err(CliParseError::InvalidArguments(format!(
            "unknown state subcommand `{other}`"
        ))),
    }
}

fn parse_state_history_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut stack: Option<String> = None;
    let mut kind: Option<String> = None;
    let mut limit: Option<usize> = None;
    let mut lineage: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--stack" => {
                stack = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--stack".to_owned(),
                    },
                )?);
            }
            "--kind" => {
                kind = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--kind".to_owned(),
                    },
                )?);
            }
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
            "--lineage" => {
                lineage = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--lineage".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if stack.is_none() => stack = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected state history argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::History {
            stack: stack.ok_or_else(|| {
                CliParseError::InvalidArguments(
                    "`state history` requires `--stack <NAME>`".to_owned(),
                )
            })?,
            kind,
            limit,
            lineage,
        },
        repo_override,
        output_json,
    }))
}

fn parse_state_plan_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut manifest: Option<PathBuf> = None;
    let mut stack: Option<String> = None;
    let mut write_report = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--write-report" => write_report = true,
            "--manifest" => {
                manifest = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--manifest".to_owned(),
                    },
                )?));
            }
            "--stack" => {
                stack = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--stack".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if manifest.is_none() && stack.is_none() => {
                if looks_like_manifest_path(&arg) {
                    manifest = Some(PathBuf::from(arg));
                } else {
                    stack = Some(arg);
                }
            }
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra state plan argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::Plan {
            manifest,
            stack,
            write_report,
        },
        repo_override,
        output_json,
    }))
}

fn parse_state_apply_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut manifest: Option<PathBuf> = None;
    let mut stack: Option<String> = None;
    let mut yes = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--manifest" => {
                manifest = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--manifest".to_owned(),
                    },
                )?));
            }
            "--stack" => {
                stack = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--stack".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if manifest.is_none() && stack.is_none() => {
                if looks_like_manifest_path(&arg) {
                    manifest = Some(PathBuf::from(arg));
                } else {
                    stack = Some(arg);
                }
            }
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra state apply argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::Apply {
            manifest,
            stack,
            yes,
        },
        repo_override,
        output_json,
    }))
}

fn parse_state_capture_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut manifest: Option<PathBuf> = None;
    let mut stack: Option<String> = None;
    let mut profile: Option<String> = None;
    let mut role: Option<String> = None;
    let mut source_env: Option<String> = None;
    let mut key: Option<String> = None;
    let mut source: Option<String> = None;
    let mut destination_ref: Option<String> = None;
    let mut hook: Option<String> = None;
    let mut task: Option<String> = None;
    let mut yes = false;
    let mut push = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--manifest" => {
                manifest = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--manifest".to_owned(),
                    },
                )?));
            }
            "--stack" => {
                stack = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--stack".to_owned(),
                    },
                )?);
            }
            "--role" => {
                role = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--role".to_owned(),
                    },
                )?);
            }
            "--source-env" => {
                source_env = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--source-env".to_owned(),
                    },
                )?);
            }
            "--key" => {
                key = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--key".to_owned(),
                    },
                )?);
            }
            "--source" => {
                source = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--source".to_owned(),
                    },
                )?);
            }
            "--ref" => {
                destination_ref = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--ref".to_owned(),
                    },
                )?);
            }
            "--hook" => {
                hook = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--hook".to_owned(),
                    },
                )?);
            }
            "--task" => {
                task = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--task".to_owned(),
                    },
                )?);
            }
            "--yes" => yes = true,
            "--push" => push = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if stack.is_none() && manifest.is_none() => stack = Some(arg),
            _ if profile.is_none() => profile = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra state capture argument `{other}`"
                )));
            }
        }
    }

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::Capture {
            manifest,
            stack,
            profile,
            role,
            source_env,
            key,
            source,
            destination_ref,
            hook,
            task,
            yes,
            push,
        },
        repo_override,
        output_json,
    }))
}

fn looks_like_manifest_path(value: &str) -> bool {
    value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
        || value.ends_with(".toml")
}
