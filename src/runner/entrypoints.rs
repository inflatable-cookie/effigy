#[path = "entrypoints/dispatch.rs"]
mod dispatch;

use crate::Command;

use crate::runner::error::RunnerError;

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    dispatch::run_command(cmd)
}

pub fn resolve_command_root(cmd: &Command) -> std::path::PathBuf {
    super::command_context::resolve_command_root(cmd)
}
