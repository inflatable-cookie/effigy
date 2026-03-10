use crate::Command;

use super::super::changelog_command::run_changelog;
use super::super::doctor::run_doctor;
use super::super::execute::run_manifest_task;
use super::super::tasks_command::run_tasks;
use crate::runner::error::RunnerError;

pub(super) fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Help(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Doctor(args) => run_doctor(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}
