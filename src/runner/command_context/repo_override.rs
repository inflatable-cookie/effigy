use std::path::PathBuf;

use effigy_cli::Command;

pub(in crate::runner) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Version => None,
        Command::Bundle(_) => None,
        Command::Changelog(_) => None,
        Command::Deploy(args) => args.repo_override.clone(),
        Command::Defer(args) => args.repo_override.clone(),
        Command::Exec(args) => args.repo_override.clone(),
        Command::System(args) => args.repo_override.clone(),
        Command::Workspace(args) => args.repo_override.clone(),
        Command::Gateway(_) => None,
        Command::Service(args) => args.repo_override.clone(),
        Command::Demo(args) => args.repo_override.clone(),
        Command::Docs(args) => args.repo_override.clone(),
        Command::Contracts(args) => args.repo_override.clone(),
        Command::Distribution(args) => args.repo_override.clone(),
        Command::Container(args) => args.repo_override.clone(),
        Command::Bootstrap(_) => None,
        Command::Release(args) => args.repo_override.clone(),
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::InternalGateway(_) => None,
        Command::InternalRhai(_) => None,
        Command::InternalContainerLeaseReaper(_) => None,
        Command::InternalHostProcessSupervise(_) => None,
        Command::InternalHostProcessStop(_) => None,
        Command::Task(_) => super::task_repo_override(cmd),
        Command::Help(_) => None,
    }
}
