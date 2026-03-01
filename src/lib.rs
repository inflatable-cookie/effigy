pub mod process_manager;
pub mod resolver;
pub mod runner;
pub mod tasks;
pub mod testing;
pub mod tui;
pub mod ui;

use std::path::{Path, PathBuf};
use ui::{Renderer, UiResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Doctor(DoctorArgs),
    Tasks(TasksArgs),
    Task(TaskInvocation),
    Help(HelpTopic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelpTopic {
    General,
    Doctor,
    Tasks,
    Test,
    Watch,
    Init,
    Migrate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorArgs {
    pub repo_override: Option<PathBuf>,
    pub output_json: bool,
    pub fix: bool,
    pub verbose: bool,
    pub explain: Option<TaskInvocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
    pub resolve_selector: Option<String>,
    pub output_json: bool,
    pub pretty_json: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInvocation {
    pub name: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliParseError {
    MissingRepoValue,
    MissingTaskNameValue,
    MissingResolveSelectorValue,
    MissingPrettyValue,
    InvalidPrettyValue(String),
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::MissingResolveSelectorValue => write!(f, "--resolve requires a value"),
            CliParseError::MissingPrettyValue => {
                write!(f, "--pretty requires a value (`true` or `false`)")
            }
            CliParseError::InvalidPrettyValue(value) => write!(
                f,
                "--pretty value `{value}` is invalid (expected `true` or `false`)"
            ),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn strip_global_json_flags(args: Vec<String>) -> (Vec<String>, bool) {
    let mut stripped = Vec::with_capacity(args.len());
    let mut json_mode = false;
    let mut passthrough_mode = false;
    for arg in args {
        if arg == "--" {
            passthrough_mode = true;
            stripped.push(arg);
            continue;
        }
        if !passthrough_mode && arg == "--json" {
            json_mode = true;
            continue;
        }
        stripped.push(arg);
    }
    (stripped, json_mode)
}

pub fn strip_global_json_flag(args: Vec<String>) -> (Vec<String>, bool) {
    strip_global_json_flags(args)
}

pub fn apply_global_json_flag(mut cmd: Command, json_mode: bool) -> Command {
    if !json_mode {
        return cmd;
    }

    match &mut cmd {
        Command::Task(task) => {
            if !task.args.iter().any(|arg| arg == "--json") {
                task.args.insert(0, "--json".to_owned());
            }
        }
        Command::Tasks(args) => args.output_json = true,
        Command::Doctor(args) => args.output_json = true,
        Command::Help(_) => {}
    }
    cmd
}

pub fn command_requests_json(cmd: &Command, global_json_mode: bool) -> bool {
    if global_json_mode {
        return true;
    }
    match cmd {
        Command::Tasks(args) => args.output_json,
        Command::Doctor(args) => args.output_json,
        Command::Task(task) => task.args.iter().any(|arg| arg == "--json"),
        Command::Help(_) => false,
    }
}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(cmd) = args.next() else {
        return Ok(Command::Help(HelpTopic::General));
    };

    if cmd == "--help" || cmd == "-h" {
        return Ok(Command::Help(HelpTopic::General));
    }
    if cmd.starts_with('-') {
        return Err(CliParseError::UnknownArgument(cmd));
    }
    if cmd == "help" {
        return Ok(Command::Help(HelpTopic::General));
    }

    if cmd == "doctor" {
        return parse_doctor(args);
    }
    if cmd == "tasks" {
        return parse_tasks(args);
    }
    if cmd == "catalogs" {
        return parse_tasks(args);
    }
    if cmd == "test" {
        let task_args = args.collect::<Vec<String>>();
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(HelpTopic::Test));
        }
        return Ok(Command::Task(TaskInvocation {
            name: cmd,
            args: task_args,
        }));
    }
    if cmd == "watch" {
        let task_args = args.collect::<Vec<String>>();
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(HelpTopic::Watch));
        }
        return Ok(Command::Task(TaskInvocation {
            name: cmd,
            args: task_args,
        }));
    }
    if cmd == "init" {
        let task_args = args.collect::<Vec<String>>();
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(HelpTopic::Init));
        }
        return Ok(Command::Task(TaskInvocation {
            name: cmd,
            args: task_args,
        }));
    }
    if cmd == "migrate" {
        let task_args = args.collect::<Vec<String>>();
        if task_args.iter().any(|arg| arg == "--help" || arg == "-h") {
            return Ok(Command::Help(HelpTopic::Migrate));
        }
        return Ok(Command::Task(TaskInvocation {
            name: cmd,
            args: task_args,
        }));
    }

    Ok(Command::Task(TaskInvocation {
        name: cmd,
        args: args.collect(),
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
            "--repo" => {
                let Some(path) = args.next() else {
                    return Err(CliParseError::MissingRepoValue);
                };
                repo_override = Some(PathBuf::from(path));
            }
            "--task" => {
                let Some(name) = args.next() else {
                    return Err(CliParseError::MissingTaskNameValue);
                };
                task_name = Some(name);
            }
            "--resolve" => {
                let Some(selector) = args.next() else {
                    return Err(CliParseError::MissingResolveSelectorValue);
                };
                resolve_selector = Some(selector);
            }
            "--json" => {
                output_json = true;
            }
            "--pretty" => {
                let Some(value) = args.next() else {
                    return Err(CliParseError::MissingPrettyValue);
                };
                pretty_json = match value.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => return Err(CliParseError::InvalidPrettyValue(value)),
                };
            }
            "--help" | "-h" => return Ok(Command::Help(HelpTopic::Tasks)),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
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
            "--repo" => {
                let Some(path) = args.next() else {
                    return Err(CliParseError::MissingRepoValue);
                };
                repo_override = Some(PathBuf::from(path));
            }
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

mod cli_help;

pub fn render_help<R: Renderer>(renderer: &mut R, topic: HelpTopic) -> UiResult<()> {
    cli_help::render_help(renderer, topic)
}

pub fn render_cli_header<R: Renderer>(renderer: &mut R, root: &Path) -> UiResult<()> {
    cli_help::render_cli_header(renderer, root)
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
