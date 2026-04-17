#[path = "util/parsing.rs"]
mod parsing;

use crate::runner::error::RunnerError;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

pub(super) fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    parsing::parse_task_runtime_args(args)
}

pub(super) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    parsing::parse_task_reference_invocation(raw)
}

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    parsing::render_passthrough_args(args)
}
