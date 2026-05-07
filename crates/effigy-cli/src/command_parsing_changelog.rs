use std::path::PathBuf;

use crate::value_parsing::next_required_value;
use crate::{ChangelogArgs, ChangelogSubcommand, CliParseError, Command, HelpTopic};

use super::unknown_argument;

pub(super) fn parse_changelog_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Changelog));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Changelog)),
        "validate" => parse_changelog_validate(args),
        "format" => parse_changelog_format(args),
        "analyze" => parse_changelog_analyze(args),
        "extract" => parse_changelog_extract(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_changelog_validate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Validate,
        file,
        output_json,
    }))
}

fn parse_changelog_format<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;
    let mut write = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--write" => write = true,
            "--preview" => write = false,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Format { write },
        file,
        output_json,
    }))
}

fn parse_changelog_analyze<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;

    for arg in args {
        match arg.as_str() {
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Analyze,
        file,
        output_json,
    }))
}

fn parse_changelog_extract<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut file: Option<PathBuf> = None;
    let mut output_json = false;
    let mut version: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => output_json = true,
            "--version" => {
                version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Changelog)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    let version = version.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "--version".to_owned(),
    })?;

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Extract { version },
        file,
        output_json,
    }))
}
