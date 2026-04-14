use std::path::PathBuf;

use crate::{
    BootstrapArgs, ChangelogArgs, ChangelogSubcommand, Command, ContractsArgs, ContractsCheckMode,
    ContractsSelectionPrintMode, ContractsSubcommand, DemoArgs, DemoHistoryOutcome, DemoListGap,
    DemoListGroupBy, DemoListMode, DemoListQuery, DemoListStatus, DemoSubcommand, DistributionArgs,
    DistributionSubcommand, DocsArgs, DocsBlockRequirement, DocsSubcommand, DoctorArgs, HelpTopic,
    InternalRhaiArgs, ReleaseArgs, ReleaseSubcommand, TaskInvocation, TasksArgs,
};

use super::value_parsing::{next_required_value, parse_pretty_bool, parse_repo_path};
use super::{unknown_argument, CliParseError};

pub(super) fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(cmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::General));
    };

    match cmd.as_str() {
        "--version" | "version" => parse_version_command(args),
        "--help" | "-h" | "help" => Ok(Command::Help(HelpTopic::General)),
        "changelog" => parse_changelog_command(args),
        "demo" => parse_demo_command(args),
        "docs" => parse_docs_command(args),
        "contracts" => parse_contracts_command(args),
        "distribution" => parse_distribution_command(args),
        "bootstrap" => parse_bootstrap(args),
        "release" => parse_release(args),
        "doctor" => parse_doctor(args),
        "tasks" | "catalogs" => parse_tasks(args),
        "__rhai-step" => parse_internal_rhai_command(args),
        _ if cmd.starts_with('-') => Err(unknown_argument(cmd)),
        _ => parse_task_command(cmd, args),
    }
}

fn parse_internal_rhai_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut file = None;
    let mut repo_root = None;
    let mut task_name = None;
    let mut script_args = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--file" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--file".to_owned(),
                    },
                )?;
                file = Some(PathBuf::from(value));
            }
            "--repo-root" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-root".to_owned(),
                    },
                )?;
                repo_root = Some(PathBuf::from(value));
            }
            "--task-name" => {
                let value = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--task-name".to_owned(),
                    },
                )?;
                task_name = Some(value);
            }
            "--" => {
                script_args.extend(args);
                break;
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            other => script_args.push(other.to_owned()),
        }
    }

    let Some(file) = file else {
        return Err(CliParseError::MissingFlagValue {
            flag: "--file".to_owned(),
        });
    };

    Ok(Command::InternalRhai(InternalRhaiArgs {
        file,
        repo_root,
        task_name,
        args: script_args,
    }))
}

fn parse_version_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(arg) = args.next() else {
        return Ok(Command::Version);
    };

    match arg.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::General)),
        other => Err(unknown_argument(other)),
    }
}

fn builtin_help_topic(cmd: &str) -> Option<HelpTopic> {
    match cmd {
        "test" => Some(HelpTopic::Test),
        "watch" => Some(HelpTopic::Watch),
        "init" => Some(HelpTopic::Init),
        "migrate" => Some(HelpTopic::Migrate),
        "demo" => Some(HelpTopic::Demo),
        "docs" => Some(HelpTopic::Docs),
        "contracts" => Some(HelpTopic::Contracts),
        "distribution" => Some(HelpTopic::Distribution),
        "bootstrap" => Some(HelpTopic::Bootstrap),
        "release" => Some(HelpTopic::Release),
        _ => None,
    }
}

fn parse_demo_command<I>(args: I) -> Result<Command, CliParseError>
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

fn parse_bootstrap<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_url: Option<String> = None;
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    let mut start = false;
    let mut plan = false;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Bootstrap)),
            "--json" => output_json = true,
            "--start" => start = true,
            "--plan" => plan = true,
            "--path" => {
                path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--path".to_owned(),
                    },
                )?));
            }
            "--branch" => {
                branch = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--branch".to_owned(),
                    },
                )?);
            }
            other if other.starts_with('-') => return Err(unknown_argument(other)),
            _ if repo_url.is_none() => repo_url = Some(arg),
            _ => return Err(unknown_argument(arg)),
        }
    }

    let Some(repo_url) = repo_url else {
        if plan || path.is_some() || branch.is_some() || start || output_json {
            return Err(CliParseError::MissingFlagValue {
                flag: "<GIT_URL>".to_owned(),
            });
        }
        return Ok(Command::Help(HelpTopic::Bootstrap));
    };

    Ok(Command::Bootstrap(BootstrapArgs {
        repo_url,
        path,
        branch,
        start,
        plan,
        output_json,
    }))
}

fn parse_docs_command<I>(args: I) -> Result<Command, CliParseError>
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

fn parse_contracts_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Contracts));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Contracts)),
        "validate-selection" => parse_contracts_validate_selection(args),
        "check-json" => parse_contracts_check_json(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_contracts_validate_selection<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut contract_path: Option<PathBuf> = None;
    let mut artifact_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--contract" => {
                contract_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--contract".to_owned(),
                    },
                )?));
            }
            "--artifact" => {
                artifact_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifact".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Contracts)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::ValidateSelection {
            contract_path,
            artifact_path,
        },
        repo_override,
        output_json,
    }))
}

fn parse_contracts_check_json<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut index_path: Option<PathBuf> = None;
    let mut mode = ContractsCheckMode::Full;
    let mut changed_only_base: Option<String> = None;
    let mut print_selected = ContractsSelectionPrintMode::None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--index" => {
                index_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--index".to_owned(),
                    },
                )?));
            }
            "--fast" => mode = ContractsCheckMode::Fast,
            "--full" => mode = ContractsCheckMode::Full,
            "--changed-only" | "--changed-only-base" => {
                changed_only_base = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue { flag: arg },
                )?);
            }
            "--print-selected" | "--print-selected=text" => {
                print_selected = ContractsSelectionPrintMode::Text;
            }
            "--print-selected=json" => {
                print_selected = ContractsSelectionPrintMode::Json;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Contracts)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Contracts(ContractsArgs {
        subcommand: ContractsSubcommand::CheckJson {
            index_path,
            mode,
            changed_only_base,
            print_selected,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Distribution));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Distribution)),
        "validate-metadata" => parse_distribution_validate_metadata(args),
        "preflight" => parse_distribution_preflight(args),
        "validate-artifacts" => parse_distribution_validate_artifacts(args),
        "generate-closeout" => parse_distribution_generate_closeout(args),
        "write-summary" => parse_distribution_write_summary(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_distribution_validate_metadata<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::ValidateMetadata { tag },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_preflight<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut skip_docs = false;
    let mut skip_smoke = false;
    let mut output_path: Option<PathBuf> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--skip-docs" => skip_docs = true,
            "--skip-smoke" => skip_smoke = true,
            "--output" => {
                output_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--output".to_owned(),
                    },
                )?));
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::Preflight {
            tag,
            skip_docs,
            skip_smoke,
            output_path,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_validate_artifacts<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut expect_homebrew = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--expect-homebrew" => expect_homebrew = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    let artifacts_dir = artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
        flag: "--artifacts-dir".to_owned(),
    })?;

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::ValidateArtifacts {
            artifacts_dir,
            expect_homebrew,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_generate_closeout<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut output_path: Option<PathBuf> = None;
    let mut owner = "Effigy".to_owned();
    let mut expect_homebrew = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--output" => {
                output_path = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--output".to_owned(),
                    },
                )?));
            }
            "--owner" => {
                owner = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--owner".to_owned(),
                    },
                )?;
            }
            "--expect-homebrew" => expect_homebrew = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::GenerateCloseout {
            tag: tag.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--tag".to_owned(),
            })?,
            artifacts_dir: artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--artifacts-dir".to_owned(),
            })?,
            output_path,
            owner,
            expect_homebrew,
        },
        repo_override,
        output_json,
    }))
}

fn parse_distribution_write_summary<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut artifacts_dir: Option<PathBuf> = None;
    let mut crate_version: Option<String> = None;
    let mut repo_url = "https://github.com/inflatable-cookie/effigy.git".to_owned();
    let mut brew_formula = "inflatable-cookie/effigy/effigy".to_owned();
    let mut homebrew_executed = false;
    let mut log_files = Vec::new();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--artifacts-dir" => {
                artifacts_dir = Some(PathBuf::from(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--artifacts-dir".to_owned(),
                    },
                )?));
            }
            "--crate-version" => {
                crate_version = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--crate-version".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?;
            }
            "--brew-formula" => {
                brew_formula = next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--brew-formula".to_owned(),
                    },
                )?;
            }
            "--homebrew-executed" => homebrew_executed = true,
            "--log-file" => log_files.push(next_required_value(
                &mut args,
                CliParseError::MissingFlagValue {
                    flag: "--log-file".to_owned(),
                },
            )?),
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Distribution)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Distribution(DistributionArgs {
        subcommand: DistributionSubcommand::WriteSummary {
            tag: tag.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--tag".to_owned(),
            })?,
            artifacts_dir: artifacts_dir.ok_or_else(|| CliParseError::MissingFlagValue {
                flag: "--artifacts-dir".to_owned(),
            })?,
            crate_version,
            repo_url,
            brew_formula,
            homebrew_executed,
            log_files,
        },
        repo_override,
        output_json,
    }))
}

fn parse_tasks<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--task" => {
                task_name = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingTaskNameValue,
                )?);
            }
            "--resolve" => {
                resolve_selector = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingResolveSelectorValue,
                )?);
            }
            "--json" => {
                output_json = true;
            }
            "--pretty" => {
                let value = next_required_value(&mut args, CliParseError::MissingPrettyValue)?;
                pretty_json = parse_pretty_bool(value)?;
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
        resolve_selector,
        output_json,
        pretty_json,
    }))
}

fn parse_doctor<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut fix = false;
    let mut verbose = false;
    let mut explain: Option<TaskInvocation> = None;

    while let Some(arg) = args.next() {
        if let Some(request) = explain.as_mut() {
            request.args.push(arg);
            continue;
        }
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--fix" => fix = true,
            "--verbose" => verbose = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Doctor)),
            other => {
                explain = Some(TaskInvocation {
                    name: other.to_owned(),
                    args: Vec::new(),
                })
            }
        }
    }

    Ok(Command::Doctor(DoctorArgs {
        repo_override,
        output_json,
        fix,
        verbose,
        explain,
    }))
}

fn parse_release<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(subcmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::Release));
    };

    match subcmd.as_str() {
        "--help" | "-h" => Ok(Command::Help(HelpTopic::Release)),
        "status" => parse_release_status(args),
        "gates" => parse_release_gates(args),
        "resume" => parse_release_resume(args),
        "verify-install" => parse_release_verify_install(args),
        "simulate" => parse_release_simulate(args),
        "prepare" => parse_release_prepare(args),
        "execute" => parse_release_execute(args),
        other => Err(unknown_argument(other)),
    }
}

fn parse_release_status<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Status { check_gates },
        repo_override,
        output_json,
    }))
}

fn parse_release_gates<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Gates,
        repo_override,
        output_json,
    }))
}

fn parse_release_resume<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Resume { allow_stale },
        repo_override,
        output_json,
    }))
}

fn parse_release_simulate<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--version" => {
                version_override = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Simulate { version_override },
        repo_override,
        output_json,
    }))
}

fn parse_release_verify_install<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut tag: Option<String> = None;
    let mut repo_url: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--tag" => {
                tag = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--tag".to_owned(),
                    },
                )?);
            }
            "--repo-url" => {
                repo_url = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--repo-url".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::VerifyInstall { tag, repo_url },
        repo_override,
        output_json,
    }))
}

fn parse_release_prepare<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut check_gates = false;
    let mut plan = false;
    let mut yes = false;
    let mut version_override: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--check-gates" => check_gates = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
            "--version" => {
                version_override = Some(next_required_value(
                    &mut args,
                    CliParseError::MissingFlagValue {
                        flag: "--version".to_owned(),
                    },
                )?);
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Prepare {
            plan,
            check_gates,
            yes,
            version_override,
        },
        repo_override,
        output_json,
    }))
}

fn parse_release_execute<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut output_json = false;
    let mut plan = false;
    let mut yes = false;
    let mut allow_stale = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => repo_override = Some(parse_repo_path(&mut args)?),
            "--json" => output_json = true,
            "--plan" | "--dry-run" => plan = true,
            "--yes" => yes = true,
            "--allow-stale" => allow_stale = true,
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Release)),
            other => return Err(unknown_argument(other)),
        }
    }

    Ok(Command::Release(ReleaseArgs {
        subcommand: ReleaseSubcommand::Execute {
            plan,
            yes,
            allow_stale,
        },
        repo_override,
        output_json,
    }))
}

fn parse_changelog_command<I>(args: I) -> Result<Command, CliParseError>
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

fn parse_task_command<I>(name: String, args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let task_args = args.into_iter().collect::<Vec<String>>();
    if let Some(topic) = builtin_help_topic(&name) {
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(topic));
        }
    }
    Ok(Command::Task(TaskInvocation {
        name,
        args: task_args,
    }))
}
