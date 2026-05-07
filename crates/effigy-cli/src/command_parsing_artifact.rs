use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{
    unknown_argument, ArtifactArgs, ArtifactSubcommand, CliParseError, Command, HelpTopic,
};

pub(super) fn parse_artifact_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcommand) = args.next() else {
        return Ok(Command::Help(HelpTopic::Artifact));
    };

    match subcommand.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Artifact)),
        "inspect" => parse_artifact_source_command(args, ArtifactVerb::Inspect),
        "stage" => parse_artifact_source_command(args, ArtifactVerb::Stage),
        "capture" => parse_artifact_capture_command(args),
        other if other.starts_with('-') => Err(unknown_argument(other)),
        other => Err(CliParseError::InvalidArguments(format!(
            "unknown artifact subcommand `{other}`"
        ))),
    }
}

fn parse_artifact_capture_command<I>(args: I) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Artifact)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if source.is_none() => source = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra artifact capture argument `{other}`"
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

    Ok(Command::Artifact(ArtifactArgs {
        subcommand: ArtifactSubcommand::Capture {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArtifactVerb {
    Inspect,
    Stage,
}

fn parse_artifact_source_command<I>(args: I, verb: ArtifactVerb) -> Result<Command, CliParseError>
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Artifact)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if source.is_none() => source = Some(arg),
            other => {
                return Err(CliParseError::InvalidArguments(format!(
                    "unexpected extra artifact argument `{other}`"
                )));
            }
        }
    }

    let source = source.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "<REF|PATH>".to_owned(),
    })?;
    let subcommand = match verb {
        ArtifactVerb::Inspect => ArtifactSubcommand::Inspect {
            source,
            farmyard_handoff,
        },
        ArtifactVerb::Stage => ArtifactSubcommand::Stage {
            source,
            farmyard_handoff,
        },
    };

    Ok(Command::Artifact(ArtifactArgs {
        subcommand,
        repo_override,
        output_json,
    }))
}
