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
    if let Some(lease_id) = super::super::admission::scoped_lease_id() {
        process.env("EFFIGY_ADMISSION_LEASE_ID", lease_id);
        if let Some(cpu_units) = super::super::admission::scoped_cpu_units() {
            process.env("CARGO_BUILD_JOBS", cpu_units.to_string());
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
        }
    } else if super::super::admission::signal_scope_active() {
        // A scheduler-launched run owns this child's group so a termination
        // signal can be forwarded to it without touching any other group.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            process.process_group(0);
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
