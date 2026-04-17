use std::path::Path;

use effigy_cli::HelpTopic;
use effigy_cli::TaskInvocation;

use super::command_spec::run_passthrough_builtin_command;
use super::render_builtin_help_topic;
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use effigy_tasks::TaskRuntimeArgs;

mod output;
#[path = "watch/request.rs"]
mod request;
mod runtime;
mod scan;
#[path = "watch/test_support.rs"]
pub(crate) mod test_support;

use request::parse_watch_request;
use runtime::run_watch_runtime;

pub(super) fn run_builtin_watch(
    ports: &dyn BuiltinRuntimePorts,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    run_passthrough_builtin_command(
        &task.name,
        runtime_args,
        |output_json| render_builtin_help_topic(HelpTopic::Watch, "watch", output_json),
        |args| parse_watch_request(task, args),
        |request| run_watch_request(ports, request, target_root),
    )
}

fn run_watch_request(
    ports: &dyn BuiltinRuntimePorts,
    request: request::WatchRequest,
    target_root: &Path,
) -> Result<Option<String>, BuiltinError> {
    request.validate_execution_policy()?;
    run_watch_runtime(ports, request, target_root)
}
