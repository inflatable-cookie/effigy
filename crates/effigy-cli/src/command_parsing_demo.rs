use std::path::PathBuf;

use crate::{
    Command, DemoArgs, DemoHistoryOutcome, DemoListGap, DemoListGroupBy, DemoListMode,
    DemoListQuery, DemoListStatus, DemoSubcommand, HelpTopic,
};

use crate::value_parsing::{next_required_value, parse_repo_path};

use super::{unknown_argument, CliParseError};

pub(super) fn parse_demo_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Demo));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Demo)),
        "browser" => parse_demo_browser(args),
        "list" => parse_demo_list(args),
        "inspect" => parse_demo_inspect(args),
        "history" => parse_demo_history(args),
        "run" => parse_demo_run(args),
        "stop" => parse_demo_stop(args),
        "input" => parse_demo_input(args),
        "resize" => parse_demo_resize(args),
        "rerun" => parse_demo_rerun(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_demo_browser<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut group_by: Option<DemoListGroupBy> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--group-by" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--group-by".to_owned(),
                    },
                )?;
                group_by = Some(parse_demo_list_group_by(value)?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Browser { group_by },
        repo_override,
        output_json,
    }))
}

fn parse_demo_list<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut query = DemoListQuery::default();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--search" => {
                query.search = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--search".to_owned(),
                    },
                )?);
            }
            "--owner" => {
                query.owner = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--owner".to_owned(),
                    },
                )?);
            }
            "--tag" => {
                query.tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--mode" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--mode".to_owned(),
                    },
                )?;
                query.mode = Some(parse_demo_list_mode(value)?);
            }
            "--cover" => {
                query.cover = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--cover".to_owned(),
                    },
                )?);
            }
            "--status" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--status".to_owned(),
                    },
                )?;
                query.status = Some(parse_demo_list_status(value)?);
            }
            "--gap" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--gap".to_owned(),
                    },
                )?;
                query.gap = Some(parse_demo_list_gap(value)?);
            }
            "--stale-only" => query.stale_only = true,
            "--group-by" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--group-by".to_owned(),
                    },
                )?;
                query.group_by = Some(parse_demo_list_group_by(value)?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ => return Err(unknown_argument(arg)),
        }
    }

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::List { query },
        repo_override,
        output_json,
    }))
}

fn parse_demo_list_mode(value: String) -> Result<DemoListMode, CliParseError> {
    match value.as_str() {
        "headless" => Ok(DemoListMode::Headless),
        "interactive" => Ok(DemoListMode::Interactive),
        "hybrid" => Ok(DemoListMode::Hybrid),
        _ => Err(CliParseError::InvalidFlagValue {
            flag: "--mode".to_owned(),
            value,
            expected: "`headless`, `interactive`, or `hybrid`".to_owned(),
        }),
    }
}

fn parse_demo_list_status(value: String) -> Result<DemoListStatus, CliParseError> {
    match value.as_str() {
        "planned" => Ok(DemoListStatus::Planned),
        "ready" => Ok(DemoListStatus::Ready),
        "running" => Ok(DemoListStatus::Running),
        "passed" => Ok(DemoListStatus::Passed),
        "failed" => Ok(DemoListStatus::Failed),
        "broken" => Ok(DemoListStatus::Broken),
        "missing" => Ok(DemoListStatus::Missing),
        _ => Err(CliParseError::InvalidFlagValue {
            flag: "--status".to_owned(),
            value,
            expected: "`planned`, `ready`, `running`, `passed`, `failed`, `broken`, or `missing`"
                .to_owned(),
        }),
    }
}

fn parse_demo_list_gap(value: String) -> Result<DemoListGap, CliParseError> {
    match value.as_str() {
        "existing" => Ok(DemoListGap::Existing),
        "planned" => Ok(DemoListGap::Planned),
        "missing" => Ok(DemoListGap::Missing),
        "broken" => Ok(DemoListGap::Broken),
        "stale" => Ok(DemoListGap::Stale),
        _ => Err(CliParseError::InvalidFlagValue {
            flag: "--gap".to_owned(),
            value,
            expected: "`existing`, `planned`, `missing`, `broken`, or `stale`".to_owned(),
        }),
    }
}

fn parse_demo_list_group_by(value: String) -> Result<DemoListGroupBy, CliParseError> {
    match value.as_str() {
        "owner" => Ok(DemoListGroupBy::Owner),
        "tag" => Ok(DemoListGroupBy::Tag),
        "mode" => Ok(DemoListGroupBy::Mode),
        "cover" => Ok(DemoListGroupBy::Cover),
        "status" => Ok(DemoListGroupBy::Status),
        "gap" => Ok(DemoListGroupBy::Gap),
        _ => Err(CliParseError::InvalidFlagValue {
            flag: "--group-by".to_owned(),
            value,
            expected: "`owner`, `tag`, `mode`, `cover`, `status`, or `gap`".to_owned(),
        }),
    }
}

fn parse_demo_inspect<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Inspect { demo_id },
        repo_override,
        output_json,
    }))
}

fn parse_demo_history<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut limit: Option<usize> = None;
    let mut outcome: Option<DemoHistoryOutcome> = None;
    let mut attempt_id: Option<String> = None;
    let mut attempt_ordinal: Option<usize> = None;
    let mut demo_id: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--limit" => {
                limit = Some(parse_positive_integer(&mut args, "--limit")?);
            }
            "--outcome" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--outcome".to_owned(),
                    },
                )?;
                outcome = Some(parse_demo_history_outcome(value)?);
            }
            "--attempt" => {
                attempt_id = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--attempt".to_owned(),
                    },
                )?);
            }
            "--ordinal" => {
                attempt_ordinal = Some(parse_positive_integer(&mut args, "--ordinal")?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::History {
            demo_id,
            limit,
            outcome,
            attempt_id,
            attempt_ordinal,
        },
        repo_override,
        output_json,
    }))
}

fn parse_demo_run<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Run { demo_id },
        repo_override,
        output_json,
    }))
}

fn parse_demo_history_outcome(value: String) -> Result<DemoHistoryOutcome, CliParseError> {
    match value.as_str() {
        "passed" => Ok(DemoHistoryOutcome::Passed),
        "failed" => Ok(DemoHistoryOutcome::Failed),
        "terminated" => Ok(DemoHistoryOutcome::Terminated),
        _ => Err(CliParseError::InvalidFlagValue {
            flag: "--outcome".to_owned(),
            value,
            expected: "`passed`, `failed`, or `terminated`".to_owned(),
        }),
    }
}

fn parse_positive_integer<I>(args: &mut I, flag: &str) -> Result<usize, CliParseError>
where
    I: Iterator<Item = String>,
{
    let value = next_required_value(
        args,
        CliParseError::MissingFlagValue {
            flag: flag.to_owned(),
        },
    )?;
    let parsed = value
        .parse::<usize>()
        .map_err(|_| CliParseError::InvalidFlagValue {
            flag: flag.to_owned(),
            value: value.clone(),
            expected: "a positive integer".to_owned(),
        })?;
    if parsed == 0 {
        return Err(CliParseError::InvalidFlagValue {
            flag: flag.to_owned(),
            value,
            expected: "a positive integer".to_owned(),
        });
    }
    Ok(parsed)
}

fn parse_demo_stop<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Stop { demo_id },
        repo_override,
        output_json,
    }))
}

fn parse_demo_input<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;
    let mut text: Option<String> = None;
    let mut append_newline = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--text" => {
                text = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--text".to_owned(),
                    },
                )?)
            }
            "--append-newline" => append_newline = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };
    let Some(text) = text else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--text".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Input {
            demo_id,
            text,
            append_newline,
        },
        repo_override,
        output_json,
    }))
}

fn parse_demo_rerun<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Rerun { demo_id },
        repo_override,
        output_json,
    }))
}

fn parse_demo_resize<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut demo_id: Option<String> = None;
    let mut cols: Option<u16> = None;
    let mut rows: Option<u16> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--cols" => cols = Some(parse_terminal_dimension(&mut args, "--cols")?),
            "--rows" => rows = Some(parse_terminal_dimension(&mut args, "--rows")?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Demo)),
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if demo_id.is_none() => demo_id = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(demo_id) = demo_id else {
        return Err(CliParseError::MissingFlagValue {
            flag: "<DEMO_ID>".to_owned(),
        });
    };
    let Some(cols) = cols else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--cols".to_owned(),
        });
    };
    let Some(rows) = rows else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--rows".to_owned(),
        });
    };

    Ok(Command::Demo(DemoArgs {
        subcommand: DemoSubcommand::Resize {
            demo_id,
            cols,
            rows,
        },
        repo_override,
        output_json,
    }))
}

fn parse_terminal_dimension<I>(args: &mut I, flag: &str) -> Result<u16, CliParseError>
where
    I: Iterator<Item = String>,
{
    let parsed = parse_positive_integer(args, flag)?;
    u16::try_from(parsed).map_err(|_| CliParseError::InvalidFlagValue {
        flag: flag.to_owned(),
        value: parsed.to_string(),
        expected: "a positive integer between 1 and 65535".to_owned(),
    })
}
