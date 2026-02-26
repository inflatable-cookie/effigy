pub mod resolver;
pub mod runner;
pub mod tasks;
pub mod ui;

use std::path::PathBuf;
use ui::{Renderer, UiResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    RepoPulse(PulseArgs),
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

    if cmd == "repo-pulse" {
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

    Ok(Command::RepoPulse(PulseArgs {
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

pub fn render_help<R: Renderer>(renderer: &mut R) -> UiResult<()> {
    renderer.section("effigy")?;
    renderer.notice(
        ui::NoticeLevel::Info,
        "Unified task runner for nested and multi-repo workspaces",
    )?;

    renderer.section("Usage")?;
    renderer.bullet_list(
        "commands",
        &vec![
            "effigy <task> [task args]".to_owned(),
            "effigy <catalog>:<task> [task args]".to_owned(),
            "effigy repo-pulse [--repo <PATH>] [--verbose-root]".to_owned(),
            "effigy tasks [--repo <PATH>] [--task <TASK_NAME>]".to_owned(),
        ],
    )?;

    renderer.section("Tasks")?;
    renderer.key_values(&[
        ui::KeyValue::new("repo-pulse", "Run built-in repository pulse checks"),
        ui::KeyValue::new("tasks", "List discovered catalogs and available tasks"),
        ui::KeyValue::new("<task>", "Resolve task across discovered catalogs"),
        ui::KeyValue::new(
            "<catalog>:<task>",
            "Run a task from an explicit catalog alias",
        ),
    ])?;

    renderer.section("Options (task run)")?;
    renderer.key_values(&[
        ui::KeyValue::new("--repo <PATH>", "Override target repository path"),
        ui::KeyValue::new(
            "--verbose-root",
            "Print root + catalog resolution trace for task execution",
        ),
    ])?;

    renderer.section("Options (repo-pulse)")?;
    renderer.key_values(&[
        ui::KeyValue::new("--repo <PATH>", "Override target repository path"),
        ui::KeyValue::new("--verbose-root", "Print root resolution trace"),
    ])?;

    renderer.section("Options (tasks)")?;
    renderer.key_values(&[
        ui::KeyValue::new("--repo <PATH>", "Override target repository path"),
        ui::KeyValue::new("--task <NAME>", "Filter output to a single task name"),
    ])?;

    renderer.section("General")?;
    renderer.key_values(&[ui::KeyValue::new("-h, --help", "Print help")])?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/lib_tests.rs"]
mod tests;
