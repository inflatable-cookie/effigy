use std::path::PathBuf;

use crate::Command;

pub(in crate::runner) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Changelog(_) => None,
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::Task(_) => super::task_repo_override(cmd),
        Command::Help(_) => None,
    }
}
