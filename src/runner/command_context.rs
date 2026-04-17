#[path = "command_context/cwd.rs"]
mod cwd;
#[path = "command_context/repo_override.rs"]
mod repo_override;
#[path = "command_context/root.rs"]
mod root;
#[path = "command_context/tasks.rs"]
mod tasks;

use effigy_cli::Command;

use super::util::parse_task_runtime_args;

pub(in crate::runner) use cwd::{canonicalize_or_original, current_working_dir};
pub(super) use repo_override::command_repo_override;
pub(super) use root::{resolve_command_root, resolve_repo_root};
pub(super) use tasks::task_selection_precedence_notes;

fn task_repo_override(cmd: &Command) -> Option<std::path::PathBuf> {
    parse_task_runtime_args(match cmd {
        Command::Task(task) => &task.args,
        _ => return None,
    })
    .ok()
    .and_then(|parsed| parsed.repo_override)
}
