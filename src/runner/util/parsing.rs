#[path = "parsing/reference.rs"]
mod reference;
#[path = "parsing/runtime.rs"]
mod runtime;
#[path = "parsing/selector.rs"]
mod selector;

use super::super::model::catalog::{TaskRuntimeArgs, TaskSelector};
use crate::runner::error::RunnerError;

pub(in crate::runner) fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    runtime::normalize_builtin_test_suite(raw)
}

pub(in crate::runner) fn parse_task_runtime_args(
    args: &[String],
) -> Result<TaskRuntimeArgs, RunnerError> {
    runtime::parse_task_runtime_args(args)
}

pub(in crate::runner) fn parse_task_selector(raw: &str) -> Result<TaskSelector, RunnerError> {
    selector::parse_task_selector(raw)
}

pub(in crate::runner) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    reference::parse_task_reference_invocation(raw)
}

pub(in crate::runner) fn render_task_selector(selector: &TaskSelector) -> String {
    selector::render_task_selector(selector)
}

pub(in crate::runner) fn render_passthrough_args(args: &[String]) -> String {
    reference::render_passthrough_args(args)
}
