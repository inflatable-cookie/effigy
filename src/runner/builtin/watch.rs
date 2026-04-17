use std::path::Path;

use effigy_cli::HelpTopic;
use effigy_cli::TaskInvocation;

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use crate::runner::error::RunnerError;
use effigy_tasks::TaskRuntimeArgs;

mod output;
#[path = "watch/request.rs"]
mod request;
mod runtime;
mod scan;
#[cfg(test)]
#[path = "watch/test_support.rs"]
pub(in crate::runner) mod test_support;

use request::parse_watch_request;
use runtime::run_watch_runtime;

pub(super) fn run_builtin_watch(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| render_builtin_help_topic(HelpTopic::Watch, "watch", output_json),
        |args| parse_watch_request(task, args),
        |request| run_watch_request(request, target_root),
    )
}

fn run_watch_request(
    request: request::WatchRequest,
    target_root: &Path,
) -> Result<Option<String>, RunnerError> {
    request.validate_execution_policy()?;
    run_watch_runtime(request, target_root)
}
