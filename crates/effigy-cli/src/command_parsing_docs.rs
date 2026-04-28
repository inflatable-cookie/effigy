use std::path::PathBuf;

use crate::{Command, DocsArgs, DocsBlockRequirement, DocsSubcommand, HelpTopic};

use crate::value_parsing::{next_required_value, parse_repo_path};

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
        "check-links" => parse_docs_check_links(args),
        "check-json-examples" => parse_docs_check_json_examples(args),
        "check-headings" => parse_docs_check_headings(args),
        "check-paths" => parse_docs_check_paths(args),
        "check-contains" => parse_docs_check_contains(args),
        "check-forbidden" => parse_docs_check_forbidden(args),
        "check-index" => parse_docs_check_index(args),
        "check-next-action" => parse_docs_check_next_action(args),
        "check-workflow-paths" => parse_docs_check_workflow_paths(args),
        "add-log-index" => parse_docs_add_log_index(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_docs_check_links<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckLinks { paths },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_json_examples<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut file: Option<PathBuf> = None;
    let mut section: Option<String> = None;
    let mut min_blocks: Option<usize> = None;
    let mut required = Vec::new();
    let mut required_blocks = Vec::new();

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
                required.push(next_required_value(
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckJsonExamples {
            file,
            section,
            min_blocks,
            required,
            required_blocks,
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_headings<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();
    let mut required_headings = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--require-heading" => {
                required_headings.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--require-heading".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckHeadings {
            paths,
            required_headings,
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_contains<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();
    let mut required_text = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--require" => {
                required_text.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--require".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckContains {
            paths,
            required_text,
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_paths<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckPaths { paths },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_forbidden<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut paths = Vec::new();
    let mut forbidden_text = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--forbid" => {
                forbidden_text.push(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--forbid".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckForbidden {
            paths,
            forbidden_text,
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_index<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut policy_index: Option<String> = None;
    let mut dir: Option<PathBuf> = None;
    let mut index: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
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
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckIndex {
            policy_index,
            dir,
            index,
        },
        repo_override,
        output_json,
    }))
}

fn parse_docs_check_next_action<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut policy_name: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--policy" => {
                policy_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--policy".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckNextAction { policy_name },
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

fn parse_docs_check_workflow_paths<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut dir: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--dir" => {
                dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--dir".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Docs)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Docs(DocsArgs {
        subcommand: DocsSubcommand::CheckWorkflowPaths { dir },
        repo_override,
        output_json,
    }))
}
