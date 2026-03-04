use std::path::Path;

use crate::{HelpTopic, TaskInvocation, TasksArgs};

use super::super::{run_tasks, RunnerError, TaskRuntimeArgs};
use super::command_spec::run_builtin_command;
use super::{reject_verbose_root_for_builtin, render_builtin_help_topic};
#[path = "tasks/request.rs"]
mod request;

pub(super) fn run_builtin_tasks(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
    catalogs_compat_alias: bool,
) -> Result<Option<String>, RunnerError> {
    reject_verbose_root_for_builtin(&task.name, runtime_args)?;
    run_builtin_command(
        &runtime_args.passthrough,
        |output_json| render_builtin_help_topic(HelpTopic::Tasks, "tasks", output_json),
        || request::parse_tasks_request(task, &runtime_args.passthrough),
        |request: request::TasksRequest| {
            if !request.output_json && !request.pretty_json {
                return Err(RunnerError::task_invocation(format!(
                    "`--pretty` is only supported together with `--json` for built-in `{}`",
                    task.name
                )));
            }

            let _ = catalogs_compat_alias;
            run_tasks(TasksArgs {
                repo_override: Some(target_root.to_path_buf()),
                task_name: request.task_name,
                resolve_selector: request.resolve_selector,
                output_json: request.output_json,
                pretty_json: request.pretty_json,
            })
            .map(Some)
        },
    )
}
