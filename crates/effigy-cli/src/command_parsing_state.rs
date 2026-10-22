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
        "capture-set" => parse_state_capture_set_command(args),
        "history" => parse_state_history_command(args),
        "artifact" => parse_state_artifact_command(args),
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
    let mut skip_layers = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--yes" => yes = true,
            "--skip-layer" => {
                skip_layers.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--skip-layer".to_owned(),
                    },
                )?);
            }
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
            skip_layers,
        },
        repo_override,
        output_json,
    }))
}

fn parse_state_capture_set_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut stack: Option<String> = None;
    let mut profiles: Vec<String> = Vec::new();
    let mut key: Option<String> = None;
    let mut yes = false;
    let mut push = false;

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
            "--key" => {
                key = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--key".to_owned(),
                    },
                )?);
            }
            "--yes" => yes = true,
            "--push" => push = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if stack.is_none() => stack = Some(arg),
            _ => profiles.push(arg),
        }
    }

    let stack = stack.ok_or_else(|| {
        CliParseError::InvalidArguments(
            "`state capture-set` requires <STACK> and at least one <PROFILE>".to_owned(),
        )
    })?;
    if profiles.is_empty() {
        return Err(CliParseError::InvalidArguments(
            "`state capture-set` requires at least one <PROFILE>".to_owned(),
        ));
    }

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::CaptureSet {
            stack,
            profiles,
            key,
            yes,
            push,
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
            _ if stack.is_none() && manifest.is_none() => {
                if looks_like_manifest_path(&arg) {
                    manifest = Some(PathBuf::from(arg));
                } else {
                    stack = Some(arg);
                }
            }
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

/// `effigy state artifact inspect|stage|capture`.
///
/// These three verbs moved here from the retired top-level `effigy artifact`
/// command; `state capture` already owns layered state-stack captures, so the
/// standalone data-artifact verbs keep their own `artifact` namespace instead of
/// colliding with it.
fn parse_state_artifact_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::State));
    };

    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::State)),
        "inspect" => parse_state_artifact_source_command(args, StateArtifactVerb::Inspect),
        "stage" => parse_state_artifact_source_command(args, StateArtifactVerb::Stage),
        "capture" => parse_state_artifact_capture_command(args),
        other if other.starts_with('-') => Err(unknown_argument(other)),
        other => Err(CliParseError::InvalidArguments(format!(
            "unknown state artifact subcommand `{other}`; use `inspect`, `stage` or `capture`"
        ))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StateArtifactVerb {
    Inspect,
    Stage,
}

fn parse_state_artifact_source_command<I>(
    args: I,
    verb: StateArtifactVerb,
) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut farmyard_handoff = false;
    let mut source: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--farmyard-handoff" => farmyard_handoff = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if source.is_none() => source = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra state artifact argument `{other}`"
                )));
            }
        }
    }

    let source = source.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "<REF|PATH>".to_owned(),
    })?;
    let subcommand = match verb {
        StateArtifactVerb::Inspect => StateSubcommand::ArtifactInspect {
            source,
            farmyard_handoff,
        },
        StateArtifactVerb::Stage => StateSubcommand::ArtifactStage {
            source,
            farmyard_handoff,
        },
    };

    Ok(Command::State(StateArgs {
        subcommand,
        repo_override,
        output_json,
    }))
}

fn parse_state_artifact_capture_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut farmyard_handoff = false;
    let mut push = false;
    let mut kind: Option<String> = None;
    let mut environment_label: Option<String> = None;
    let mut destination: Option<String> = None;
    let mut source: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--farmyard-handoff" => farmyard_handoff = true,
            "--push" => push = true,
            "--ref" => {
                destination = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--ref".to_owned(),
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
            "--environment" => {
                environment_label = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--environment".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::State)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if source.is_none() => source = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra state artifact capture argument `{other}`"
                )));
            }
        }
    }

    let source = source.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "<SOURCE_PATH>".to_owned(),
    })?;
    let destination = destination.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "--ref".to_owned(),
    })?;

    Ok(Command::State(StateArgs {
        subcommand: StateSubcommand::ArtifactCapture {
            source,
            destination,
            kind,
            environment_label,
            farmyard_handoff,
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
