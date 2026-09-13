#[path = "entrypoints/dispatch.rs"]
mod dispatch;

use effigy_cli::Command;
use effigy_context::EffigyRuntimeContext;
use std::path::Path;

use crate::runner::error::RunnerError;

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    dispatch::run_command(cmd)
}

pub(in crate::runner) fn run_command_with_cwd(
    cmd: Command,
    cwd: &Path,
) -> Result<String, RunnerError> {
    dispatch::run_command_with_cwd(cmd, cwd)
}

pub fn run_command_with_context(
    cmd: Command,
    context: &EffigyRuntimeContext,
) -> Result<String, RunnerError> {
    dispatch::run_command_with_context(cmd, context)
}

/// Run a resolved `effigy skill run --stdio passthrough` command and return the
/// task's exit status.
///
/// The caller must not render Effigy output around this result: the task owns
/// raw stdin/stdout/stderr, and only preflight or launch failures surface as
/// `Err` for a stderr diagnostic.
pub fn run_skill_passthrough_with_context(
    cmd: Command,
    context: &EffigyRuntimeContext,
) -> Result<i32, RunnerError> {
    dispatch::run_skill_passthrough_with_context(cmd, context)
}

pub fn resolve_command_root(cmd: &Command) -> std::path::PathBuf {
    super::command_context::resolve_command_root(cmd)
}
