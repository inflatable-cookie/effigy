pub mod resolver;
pub mod runner;
pub mod tasks;

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Pulse(PulseArgs),
    Tasks(TasksArgs),
    Task(TaskInvocation),
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PulseArgs {
    pub repo_override: Option<PathBuf>,
    pub verbose_root: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TasksArgs {
    pub repo_override: Option<PathBuf>,
    pub task_name: Option<String>,
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
    UnknownArgument(String),
}

impl std::fmt::Display for CliParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliParseError::MissingRepoValue => write!(f, "--repo requires a value"),
            CliParseError::MissingTaskNameValue => write!(f, "--task requires a value"),
            CliParseError::UnknownArgument(arg) => write!(f, "unknown argument: {arg}"),
        }
    }
}

impl std::error::Error for CliParseError {}

pub fn parse_command<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let Some(cmd) = args.next() else {
        return Ok(Command::Help);
    };

    if cmd == "--help" || cmd == "-h" {
        return Ok(Command::Help);
    }

    if cmd == "pulse" {
        return parse_pulse(args);
    }
    if cmd == "tasks" {
        return parse_tasks(args);
    }

    Ok(Command::Task(TaskInvocation {
        name: cmd,
        args: args.collect(),
    }))
}

fn parse_pulse<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut verbose_root = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--repo" => {
                let Some(path) = args.next() else {
                    return Err(CliParseError::MissingRepoValue);
                };
                repo_override = Some(PathBuf::from(path));
            }
            "--verbose-root" => {
                verbose_root = true;
            }
            "--help" | "-h" => return Ok(Command::Help),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::Pulse(PulseArgs {
        repo_override,
        verbose_root,
    }))
}

fn parse_tasks<I>(args: I) -> Result<Command, CliParseError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut repo_override: Option<PathBuf> = None;
    let mut task_name: Option<String> = None;

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
            "--help" | "-h" => return Ok(Command::Help),
            other => return Err(CliParseError::UnknownArgument(other.to_owned())),
        }
    }

    Ok(Command::Tasks(TasksArgs {
        repo_override,
        task_name,
    }))
}

pub fn print_usage() {
    eprintln!(
        "effigy\n\nUSAGE:\n  effigy <task> [task args]\n  effigy <catalog>:<task> [task args]\n  effigy pulse [--repo <PATH>] [--verbose-root]\n  effigy tasks [--repo <PATH>] [--task <TASK_NAME>]\n\nTASKS:\n  pulse             Run the built-in repo pulse task\n  tasks             List discovered catalogs and available tasks\n  <task>            Resolve task name across discovered effigy.tasks.toml catalogs\n  <catalog>:<task>  Run a task from an explicit catalog alias\n\nOPTIONS (task run):\n  --repo <PATH>     Override target repository path\n  --verbose-root    Print root + catalog resolution trace for task execution\n\nOPTIONS (pulse):\n  --repo <PATH>     Override target repository path\n  --verbose-root    Print root resolution trace\n\nOPTIONS (tasks):\n  --repo <PATH>     Override target repository path\n  --task <NAME>     Filter output to a single task name\n\nGENERAL:\n  -h, --help        Print help\n"
    );
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
