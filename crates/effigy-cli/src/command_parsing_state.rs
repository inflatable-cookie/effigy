use super::common::{CommonArgs, CommonFlag};
use crate::value_parsing::next_required_value;
use crate::{unknown_argument, CliParseError, Command, HelpTopic, StateArgs, StateSubcommand};

/// `effigy state` keeps only `state artifact inspect|stage|capture`; v0.16
/// retired the layered stack verbs (`plan`, `apply`, `capture`, `capture-set`,
/// `history`). See `docs/knowledge/deployment-data-workflows.md`.
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
        "artifact" => parse_state_artifact_command(args),
        "plan" | "apply" | "capture" | "capture-set" | "history" => Err(CliParseError::InvalidArguments(format!(
            "`effigy state {subcommand}` was removed in v0.16: layered state stacks moved to repository tasks (see docs/knowledge/deployment-data-workflows.md). `effigy state artifact inspect|stage|capture` stays for plain data artifacts"
        ))),
        other if other.starts_with('-') => Err(unknown_argument(other)),
        other => Err(CliParseError::InvalidArguments(format!(
            "unknown state subcommand `{other}`; only `effigy state artifact` remains"
        ))),
    }
}

/// `effigy state artifact inspect|stage|capture`.
///
/// These three verbs moved here from the retired top-level `effigy artifact`
/// command and are the plain data-artifact transport that keeps its consumers.
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
    let mut common = CommonArgs::default();
    let mut farmyard_handoff = false;
    let mut source: Option<String> = None;

    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::State)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            "--farmyard-handoff" => farmyard_handoff = true,
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
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

fn parse_state_artifact_capture_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut common = CommonArgs::default();
    let mut farmyard_handoff = false;
    let mut push = false;
    let mut kind: Option<String> = None;
    let mut environment_label: Option<String> = None;
    let mut destination: Option<String> = None;
    let mut source: Option<String> = None;

    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::State)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
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
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}
