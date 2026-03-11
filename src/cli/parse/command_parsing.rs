use std::path::PathBuf;

use crate::{
    ChangelogArgs, ChangelogSubcommand, Command, DoctorArgs, HelpTopic, ReleaseArgs,
    ReleaseSubcommand, TaskInvocation, TasksArgs,
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
        "--help" | "-h" | "help" => Ok(Command::Help(HelpTopic::General)),
        "changelog" => parse_changelog_command(args),
        "release" => parse_release(args),
        "doctor" => parse_doctor(args),
        "tasks" | "catalogs" => parse_tasks(args),
        _ if cmd.starts_with('-') => Err(unknown_argument(cmd)),
        _ => parse_task_command(cmd, args),
    }
}

fn builtin_help_topic(cmd: &str) -> Option<HelpTopic> {
    match cmd {
        "test" => Some(HelpTopic::Test),
        "watch" => Some(HelpTopic::Watch),
        "init" => Some(HelpTopic::Init),
        "migrate" => Some(HelpTopic::Migrate),
        "release" => Some(HelpTopic::Release),
        _ => None,
    }
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
