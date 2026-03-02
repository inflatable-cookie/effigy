use std::path::PathBuf;

use crate::Command;

use super::util::parse_task_runtime_args;

pub(super) fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::Task(task) => parse_task_runtime_args(&task.args)
            .ok()
            .and_then(|parsed| parsed.repo_override),
        Command::Help(_) => None,
    }
}

pub(super) fn task_selection_precedence_notes() -> Vec<String> {
    [
        "explicit catalog alias prefix",
        "relative/absolute catalog path prefix",
        "unprefixed nearest in-scope catalog by cwd",
        "unprefixed shallowest catalog from workspace root",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
}
