use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{Command, GraphArgs, GraphSubcommand, HelpTopic};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_graph_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Graph));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Graph)),
        "index" => parse_graph_index(args),
        "status" => parse_graph_status(args),
        "watch" => parse_graph_watch(args),
        "search" => parse_graph_search(args),
        "files" => parse_graph_files(args),
        "node" => parse_graph_node(args),
        "callers" => parse_graph_relation(args, true),
        "callees" => parse_graph_relation(args, false),
        "impact" => parse_graph_impact(args),
        "context" => parse_graph_context(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_graph_index<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let (repo_override, output_json) = parse_common_graph_flags(args)?;
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Index,
        repo_override,
        output_json,
    }))
}

fn parse_graph_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let (repo_override, output_json) = parse_common_graph_flags(args)?;
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status,
        repo_override,
        output_json,
    }))
}

fn parse_graph_watch<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut debounce_ms = 1_000u64;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--debounce-ms" => {
                debounce_ms = parse_positive_u64(&mut args, "--debounce-ms")?;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Watch { debounce_ms },
        repo_override,
        output_json,
    }))
}

fn parse_graph_search<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut limit = None;
    let mut query = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => limit = Some(parse_limit(&mut args, "--limit")?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if query.is_some() {
                    return Err(unknown_argument(&arg));
                }
                query = Some(arg);
            }
        }
    }
    let query = query.ok_or(CliParseError::MissingFlagValue {
        flag: "<QUERY>".to_owned(),
    })?;
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Search { query, limit },
        repo_override,
        output_json,
    }))
}

fn parse_graph_files<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut limit = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => limit = Some(parse_limit(&mut args, "--limit")?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Files { limit },
        repo_override,
        output_json,
    }))
}

fn parse_graph_node<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut id = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if id.is_some() {
                    return Err(unknown_argument(&arg));
                }
                id = Some(arg);
            }
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Node {
            id: id.ok_or(CliParseError::MissingFlagValue {
                flag: "<ID>".to_owned(),
            })?,
        },
        repo_override,
        output_json,
    }))
}

fn parse_graph_relation<I>(args: I, callers: bool) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut limit = None;
    let mut id = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => limit = Some(parse_limit(&mut args, "--limit")?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if id.is_some() {
                    return Err(unknown_argument(&arg));
                }
                id = Some(arg);
            }
        }
    }
    let id = id.ok_or(CliParseError::MissingFlagValue {
        flag: "<ID>".to_owned(),
    })?;
    Ok(Command::Graph(GraphArgs {
        subcommand: if callers {
            GraphSubcommand::Callers { id, limit }
        } else {
            GraphSubcommand::Callees { id, limit }
        },
        repo_override,
        output_json,
    }))
}

fn parse_graph_impact<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut limit = None;
    let mut target = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => limit = Some(parse_limit(&mut args, "--limit")?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if target.is_some() {
                    return Err(unknown_argument(&arg));
                }
                target = Some(arg);
            }
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Impact {
            target: target.ok_or(CliParseError::MissingFlagValue {
                flag: "<TARGET>".to_owned(),
            })?,
            limit,
        },
        repo_override,
        output_json,
    }))
}

fn parse_graph_context<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut max_files = None;
    let mut max_bytes = None;
    let mut languages = Vec::new();
    let mut paths = Vec::new();
    let mut request = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--max-files" => max_files = Some(parse_limit(&mut args, "--max-files")?),
            "--max-bytes" => max_bytes = Some(parse_limit(&mut args, "--max-bytes")?),
            "--language" => languages.push(next_required_value(
                &mut args,
                CliParseError::MissingFlagValue {
                    flag: "--language".to_owned(),
                },
            )?),
            "--path" => paths.push(next_required_value(
                &mut args,
                CliParseError::MissingFlagValue {
                    flag: "--path".to_owned(),
                },
            )?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => {
                if request.is_some() {
                    return Err(unknown_argument(&arg));
                }
                request = Some(arg);
            }
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Context {
            request: request.ok_or(CliParseError::MissingFlagValue {
                flag: "<REQUEST>".to_owned(),
            })?,
            max_files,
            max_bytes,
            languages,
            paths,
        },
        repo_override,
        output_json,
    }))
}

fn parse_common_graph_flags<I>(args: I) -> Result<(Option<PathBuf>, bool), CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok((repo_override, output_json)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }
    Ok((repo_override, output_json))
}

fn parse_limit<I>(args: &mut I, flag: &str) -> Result<usize, CliParseError>
where
    I: Iterator<Item = String>,
{
    let value = next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: flag.to_owned(),
        },
    )?;
    value
        .parse::<usize>()
        .map_err(|_| CliParseError::InvalidFlagValue {
            flag: flag.to_owned(),
            value,
            expected: "a positive integer".to_owned(),
        })
}

fn parse_positive_u64<I>(args: &mut I, flag: &str) -> Result<u64, CliParseError>
where
    I: Iterator<Item = String>,
{
    let value = next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: flag.to_owned(),
        },
    )?;
    value
        .parse::<u64>()
        .ok()
        .filter(|parsed| *parsed > 0)
        .ok_or(CliParseError::InvalidFlagValue {
            flag: flag.to_owned(),
            value,
            expected: "a positive integer".to_owned(),
        })
}
