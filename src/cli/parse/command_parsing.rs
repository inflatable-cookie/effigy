use std::path::PathBuf;

use crate::{Command, DoctorArgs, HelpTopic, TaskInvocation, TasksArgs};

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
