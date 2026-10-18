use std::path::PathBuf;

use crate::value_parsing::{next_required_value, parse_repo_path};
use crate::{Command, GraphArgs, GraphSubcommand, HelpTopic};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_graph_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut positional = Vec::new();
    let mut stale_index = false;
    for arg in args {
        match arg.as_str() {
            "--catalog" | "--all-catalogs" => return Err(CliParseError::InvalidArguments(
                format!("`{arg}` was removed in v0.15; use `effigy graph` on the whole checkout; use `--path` with `graph explore` to narrow results")
            )),
            "--stale-index" => stale_index = true,
            _ => positional.push(arg),
        }
    }
    let mut args = positional.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Graph));
    };

    let mut command = match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Graph)),
        "index" => parse_graph_index(args),
        "status" => parse_graph_status(args),
        "watch" => Err(retired(
            "watch",
            "graph index (queries refresh automatically)",
        )),
        "search" => parse_graph_search(args),
        "files" => Err(retired(
            "files",
            "graph search <QUERY> or graph node <PATH>",
        )),
        "node" => parse_graph_node(args),
        "callers" => Err(retired("callers", "graph node <ID> --in")),
        "callees" => Err(retired("callees", "graph node <ID> --out")),
        "impact" => Err(retired("impact", "graph node <ID|PATH>")),
        "affected" => parse_graph_affected(args),
        "context" => Err(retired(
            "context",
            "graph explore <REQUEST> --excerpts-only",
        )),
        "explore" => parse_graph_explore(args),
        other => Err(unknown_argument(other)),
    }?;
    if let Command::Graph(graph) = &mut command {
        if stale_index
            && matches!(
                graph.subcommand,
                GraphSubcommand::Index | GraphSubcommand::Status { .. }
            )
        {
            return Err(CliParseError::InvalidArguments(
                "`--stale-index` is a data-query option; use `graph status` for a report-only check".to_owned(),
            ));
        }
        graph.stale_index = stale_index;
    }
    Ok(command)
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
        stale_index: false,
    }))
}

fn parse_graph_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut refresh = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--refresh" => refresh = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => return Err(unknown_argument(other)),
        }
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Status { refresh },
        repo_override,
        output_json,
        stale_index: false,
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
        stale_index: false,
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
    let mut incoming = false;
    let mut outgoing = false;
    let mut limit = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--in" => incoming = true,
            "--out" => outgoing = true,
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
    if incoming && outgoing {
        return Err(CliParseError::InvalidArguments(
            "`--in` and `--out` cannot be combined; omit both to show both directions".to_owned(),
        ));
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Node {
            id: id.ok_or(CliParseError::MissingFlagValue {
                flag: "<ID|PATH>".to_owned(),
            })?,
            incoming,
            outgoing,
            limit,
        },
        repo_override,
        output_json,
        stale_index: false,
    }))
}

fn parse_graph_affected<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override = None;
    let mut output_json = false;
    let mut limit = None;
    let mut depth = 2usize;
    let mut read_stdin = false;
    let mut changed_paths = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => limit = Some(parse_limit(&mut args, "--limit")?),
            "--depth" => depth = parse_limit(&mut args, "--depth")?,
            "--stdin" => read_stdin = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Graph)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => changed_paths.push(arg),
        }
    }
    if changed_paths.is_empty() && !read_stdin {
        return Err(CliParseError::MissingFlagValue {
            flag: "<PATH>... or --stdin".to_owned(),
        });
    }
    Ok(Command::Graph(GraphArgs {
        subcommand: GraphSubcommand::Affected {
            changed_paths,
            read_stdin,
            depth,
            limit,
        },
        repo_override,
        output_json,
        stale_index: false,
    }))
}

fn parse_graph_explore<I>(args: I) -> Result<Command, CliParseError>
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
    let mut excerpts_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--excerpts-only" => excerpts_only = true,
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
    let request = request.ok_or(CliParseError::MissingFlagValue {
        flag: "<REQUEST>".to_owned(),
    })?;
    let subcommand = GraphSubcommand::Explore {
        request,
        max_files,
        max_bytes,
        languages,
        paths,
        excerpts_only,
    };
    Ok(Command::Graph(GraphArgs {
        subcommand,
        repo_override,
        output_json,
        stale_index: false,
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

fn retired(command: &str, replacement: &str) -> CliParseError {
    CliParseError::InvalidArguments(format!(
        "`graph {command}` was removed in v0.15; use `effigy {replacement}`"
    ))
}
