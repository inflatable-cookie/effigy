use std::path::Path;

use crate::{TaskInvocation, TasksArgs};

use super::super::{run_tasks, RunnerError, TaskRuntimeArgs};

pub(super) fn run_builtin_tasks(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(format!(
            "`--verbose-root` is not supported for built-in `{}`",
            task.name
        )));
    }

    let mut task_name: Option<String> = None;
    let mut i = 0usize;
    while i < runtime_args.passthrough.len() {
        let arg = &runtime_args.passthrough[i];
        if arg == "--task" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "task argument --task requires a value".to_owned(),
                ));
            };
            task_name = Some(value.clone());
            i += 2;
            continue;
        }
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }

    run_tasks(TasksArgs {
        repo_override: Some(target_root.to_path_buf()),
        task_name,
    })
}
