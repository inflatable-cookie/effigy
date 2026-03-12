use std::path::PathBuf;

use crate::Command;

pub(in crate::runner) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Changelog(_) => None,
        Command::Docs(args) => args.repo_override.clone(),
        Command::Contracts(args) => args.repo_override.clone(),
        Command::Distribution(args) => args.repo_override.clone(),
        Command::Release(args) => args.repo_override.clone(),
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::Task(_) => super::task_repo_override(cmd),
        Command::Help(_) => None,
    }
}
