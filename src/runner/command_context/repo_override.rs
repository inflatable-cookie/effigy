use std::path::PathBuf;

use crate::Command;

pub(in crate::runner) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Version => None,
        Command::Changelog(_) => None,
        Command::Demo(args) => args.repo_override.clone(),
        Command::Docs(args) => args.repo_override.clone(),
        Command::Contracts(args) => args.repo_override.clone(),
        Command::Distribution(args) => args.repo_override.clone(),
        Command::Container(args) => args.repo_override.clone(),
        Command::Bootstrap(_) => None,
        Command::Release(args) => args.repo_override.clone(),
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::InternalRhai(_) => None,
        Command::Task(_) => super::task_repo_override(cmd),
        Command::Help(_) => None,
    }
}
