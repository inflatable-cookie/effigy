use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{Command, DocsArgs, DocsBlockRequirement, DocsCheckKind, DocsSubcommand, HelpTopic};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_docs_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Docs));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Docs)),
        "check" => parse_docs_check(args),
        "add-log-index" => parse_docs_add_log_index(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_docs_check<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(kind_raw) = args.next() else {
        return Err(CliParseError::InvalidArguments(
            "`docs check` requires a kind (`links`, `json-examples`, `headings`, `paths`, `contains`, `forbidden`, `index`, `next-action`, or `workflow-paths`)".to_owned(),
        ));
    };

    let kind = match kind_raw.as_str() {
        "links" => DocsCheckKind::Links,
        "json-examples" => DocsCheckKind::JsonExamples,
        "headings" => DocsCheckKind::Headings,
        "paths" => DocsCheckKind::Paths,
        "contains" => DocsCheckKind::Contains,
        "forbidden" => DocsCheckKind::Forbidden,
        "index" => DocsCheckKind::Index,
        "next-action" => DocsCheckKind::NextAction,
        "workflow-paths" => DocsCheckKind::WorkflowPaths,
        "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
        other => {
            return Err(CliParseError::InvalidArguments(format!(
                "`docs check` kind `{other}` is invalid (expected `links`, `json-examples`, `headings`, `paths`, `contains`, `forbidden`, `index`, `next-action`, or `workflow-paths`)"
            )))
        }
    };

    parse_docs_check_kind(kind, args)
}

fn parse_docs_check_kind<I>(kind: DocsCheckKind, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();
    let mut file: Option<PathBuf> = None;
    let mut section: Option<String> = None;
    let mut min_blocks: Option<usize> = None;
    let mut required_text = Vec::new();
    let mut required_blocks = Vec::new();
    let mut required_headings = Vec::new();
    let mut forbidden_text = Vec::new();
    let mut policy_index: Option<String> = None;
    let mut dir: Option<PathBuf> = None;
    let mut index: Option<PathBuf> = None;
    let mut policy_name: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--file" => {
                file = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--file".to_owned(),
                    },
                )?));
            }
            "--section" => {
                section = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--section".to_owned(),
                    },
                )?);
            }
            "--min-blocks" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--min-blocks".to_owned(),
                    },
                )?;
                min_blocks =
                    Some(
                        value
                            .parse::<usize>()
                            .map_err(|_| CliParseError::InvalidFlagValue {
                                flag: "--min-blocks".to_owned(),
                                value: value.clone(),
                                expected: "a positive integer".to_owned(),
                            })?,
                    );
            }
            "--require" => {
                required_text.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--require".to_owned(),
                    },
                )?);
            }
            "--require-block" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--require-block".to_owned(),
                    },
                )?;
                let Some((block_index, needle)) = value.split_once(':') else {
                    return Err(CliParseError::InvalidFlagValue {
                        flag: "--require-block".to_owned(),
                        value,
                        expected: "`<1-based-block-index>:<substring>`".to_owned(),
                    });
                };
                let block_index =
                    block_index
                        .parse::<usize>()
                        .map_err(|_| CliParseError::InvalidFlagValue {
                            flag: "--require-block".to_owned(),
                            value: value.clone(),
                            expected: "`<1-based-block-index>:<substring>`".to_owned(),
                        })?;
                required_blocks.push(DocsBlockRequirement {
                    block_index,
                    needle: needle.to_owned(),
                });
            }
            "--require-heading" => {
                required_headings.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--require-heading".to_owned(),
                    },
                )?);
            }
            "--forbid" => {
                forbidden_text.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--forbid".to_owned(),
                    },
                )?);
            }
            "--policy-index" => {
                policy_index = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--policy-index".to_owned(),
                    },
                )?);
            }
            "--dir" => {
                dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--dir".to_owned(),
                    },
                )?));
            }
            "--index" => {
                index = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--index".to_owned(),
                    },
                )?));
            }
            "--policy" => {
                policy_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--policy".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::Check {
            kind,
            paths,
            file,
            section,
            min_blocks,
            required_text,
            required_blocks,
            required_headings,
            forbidden_text,
            policy_index: Box::new(policy_index),
            dir: Box::new(dir),
            index: Box::new(index),
            policy_name: Box::new(policy_name),
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_add_log_index<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut log_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if log_path.is_some() {
                    return Err(unknown_argument(arg));
                }
                log_path = Some(PathBuf::from(arg));
            }
        }
    }

    let log_path = log_path.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "<LOG_FILE>".to_owned(),
    })?;

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::AddLogIndex { log_path },
        repo_override,
        output_json,
    }))
}
