use std::process::Command as ProcessCommand;

use super::super::util::with_local_node_bin_path;
use super::context::ExecutionTaskContext;
use crate::runner::error::RunnerError;

pub(super) fn build_shell_process(context: &ExecutionTaskContext<'_>) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-lc")
        .arg(context.command())
        .current_dir(context.repo_for_task());
    with_local_node_bin_path(&mut process, context.repo_for_task());
    process
}

pub(super) fn command_launch_error(
    context: &ExecutionTaskContext<'_>,
    error: std::io::Error,
) -> RunnerError {
    RunnerError::TaskCommandLaunch {
        command: context.command().to_owned(),
        error,
    }
}
