use std::path::PathBuf;

use crate::resolver::resolve_target_root;
use crate::{Command, DoctorArgs, TasksArgs};

use super::command_context::command_repo_override;
use super::execute::run_manifest_task;
use super::RunnerError;

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Help(_) => Ok(String::new()),
        Command::Doctor(args) => run_doctor(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}

pub fn resolve_command_root(cmd: &Command) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_override = command_repo_override(cmd);

    match resolve_target_root(cwd.clone(), repo_override) {
        Ok(resolved) => resolved.resolved_root,
        Err(_) => cwd,
    }
}

pub fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
    super::doctor::run_doctor(args)
}

pub fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    super::tasks_command::run_tasks(args)
}
