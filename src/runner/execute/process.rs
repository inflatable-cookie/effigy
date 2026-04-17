use std::process::Command as ProcessCommand;

use super::context::ExecutionTaskContext;
use crate::runner::error::RunnerError;
use effigy_core::shell::with_local_node_bin_path;
use effigy_env::secret::SecretString;

pub(super) fn build_shell_process(
    context: &ExecutionTaskContext<'_>,
    secret_env: Option<&[(&str, &SecretString)]>,
) -> ProcessCommand {
    let mut process = ProcessCommand::new("sh");
    process
        .arg("-c")
        .arg(context.command())
        .current_dir(context.repo_for_task());
    with_local_node_bin_path(&mut process, context.repo_for_task());
    if let Some(secrets) = secret_env {
        for (key, secret) in secrets {
            process.env(key, secret.expose());
        }
    }
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
