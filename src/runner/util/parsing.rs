#[path = "parsing/reference.rs"]
mod reference;
#[path = "parsing/runtime.rs"]
mod runtime;

use crate::runner::error::RunnerError;
use effigy_tasks::TaskRuntimeArgs;

pub(in crate::runner) fn parse_task_runtime_args(
    args: &[String],
) -> Result<TaskRuntimeArgs, RunnerError> {
    runtime::parse_task_runtime_args(args)
}

pub(in crate::runner) fn render_passthrough_args(args: &[String]) -> String {
    reference::render_passthrough_args(args)
}
