use std::path::Path;

use crate::PulseArgs;
use crate::TaskInvocation;

use super::super::{run_pulse, RunnerError, TaskRuntimeArgs};

pub(super) fn run_builtin_repo_pulse(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if !runtime_args.passthrough.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }
    run_pulse(PulseArgs {
        repo_override: Some(target_root.to_path_buf()),
        verbose_root: runtime_args.verbose_root,
    })
}
