use std::path::Path;

use crate::{TaskInvocation, TasksArgs};

use super::super::{run_tasks, RunnerError, TaskRuntimeArgs};

pub(super) fn run_builtin_tasks(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs_compat_alias: bool,
) -> Result<String, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(format!(
            "`--verbose-root` is not supported for built-in `{}`",
            task.name
        )));
    }

    let mut task_name: Option<String> = None;
    let mut resolve_selector: Option<String> = None;
    let mut output_json = false;
    let mut pretty_json = true;
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
        if arg == "--resolve" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(format!(
                    "{} argument --resolve requires a value",
                    task.name
                )));
            };
            resolve_selector = Some(value.clone());
            i += 2;
            continue;
        }
        if arg == "--json" {
            output_json = true;
            i += 1;
            continue;
        }
        if arg == "--pretty" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(format!(
                    "{} argument --pretty requires a value (`true` or `false`)",
                    task.name
                )));
            };
            pretty_json = match value.as_str() {
                "true" => true,
                "false" => false,
                _ => {
                    return Err(RunnerError::TaskInvocation(format!(
                        "{} argument --pretty value `{value}` is invalid (expected `true` or `false`)",
                        task.name
                    )));
                }
            };
            i += 2;
            continue;
        }
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }

    if !output_json && !pretty_json {
        return Err(RunnerError::TaskInvocation(format!(
            "`--pretty` is only supported together with `--json` for built-in `{}`",
            task.name
        )));
    }

    let _ = catalogs_compat_alias;
    run_tasks(TasksArgs {
        repo_override: Some(target_root.to_path_buf()),
        task_name,
        resolve_selector,
        output_json,
        pretty_json,
    })
}
