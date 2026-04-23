#[path = "util/parsing.rs"]
mod parsing;

use crate::runner::error::RunnerError;
use effigy_tasks::TaskRuntimeArgs;

pub(super) fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    parsing::parse_task_runtime_args(args)
}

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    parsing::render_passthrough_args(args)
}
