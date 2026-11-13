use std::path::PathBuf;

use crate::value_parsing::next_required_value;
use crate::{ChangelogArgs, ChangelogSubcommand, CliParseError, Command, HelpTopic};

use super::common::{CommonArgs, CommonFlag};
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
    let mut common = CommonArgs::default();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Changelog)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Validate,
        file,
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

fn parse_changelog_format<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut common = CommonArgs::default();
    let mut write = false;

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Changelog)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            "--write" => write = true,
            "--preview" => write = false,
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Format { write },
        file,
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

fn parse_changelog_analyze<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut file: Option<PathBuf> = None;
    let mut common = CommonArgs::default();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Changelog)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => file = Some(PathBuf::from(arg)),
        }
    }

    Ok(Command::Changelog(ChangelogArgs {
        subcommand: ChangelogSubcommand::Analyze,
        file,
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}

fn parse_changelog_extract<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut file: Option<PathBuf> = None;
    let mut common = CommonArgs::default();
    let mut version: Option<String> = None;

    while let Some(arg) = args.next() {
        match common.take(&arg, &mut args, HelpTopic::Changelog)? {
            CommonFlag::Help(topic) => return Ok(Command::Help(topic)),
            CommonFlag::Consumed => continue,
            CommonFlag::Unclaimed => {}
        }
        match arg.as_str() {
            "--version" => {
                version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
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
        repo_override: common.repo_override,
        output_json: common.output_json,
    }))
}
