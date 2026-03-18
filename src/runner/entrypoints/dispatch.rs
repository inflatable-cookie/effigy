use crate::Command;

use super::super::bootstrap_command::run_bootstrap;
use super::super::changelog_command::run_changelog;
use super::super::contracts_command::run_contracts;
use super::super::distribution_command::run_distribution;
use super::super::docs_command::run_docs;
use super::super::doctor::run_doctor;
use super::super::execute::run_manifest_task;
use super::super::release_command::run_release;
use super::super::tasks_command::run_tasks;
use crate::runner::error::RunnerError;

pub(super) fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Version => Ok(String::new()),
        Command::Help(_) => Ok(String::new()),
        Command::Changelog(args) => run_changelog(args),
        Command::Docs(args) => run_docs(args),
        Command::Contracts(args) => run_contracts(args),
        Command::Distribution(args) => run_distribution(args),
        Command::Bootstrap(args) => run_bootstrap(args),
        Command::Release(args) => run_release(args),
        Command::Doctor(args) => run_doctor(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}
