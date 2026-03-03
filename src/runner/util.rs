#[path = "util/parsing.rs"]
mod parsing;
#[path = "util/dotenv.rs"]
mod dotenv;
#[path = "util/shell.rs"]
mod shell;

use std::collections::BTreeMap;

pub(super) fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    parsing::normalize_builtin_test_suite(raw)
}

pub(super) fn parse_task_runtime_args(
    args: &[String],
) -> Result<super::TaskRuntimeArgs, super::RunnerError> {
    parsing::parse_task_runtime_args(args)
}

pub(super) fn parse_task_selector(raw: &str) -> Result<super::TaskSelector, super::RunnerError> {
    parsing::parse_task_selector(raw)
}

pub(super) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(super::TaskSelector, Vec<String>), super::RunnerError> {
    parsing::parse_task_reference_invocation(raw)
}

pub(super) fn render_task_selector(selector: &super::TaskSelector) -> String {
    parsing::render_task_selector(selector)
}

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    parsing::render_passthrough_args(args)
}

pub(super) fn parse_dotenv_entries(src: &str) -> BTreeMap<String, String> {
    dotenv::parse_dotenv_entries(src)
}

pub(super) fn with_local_node_bin_path(process: &mut std::process::Command, cwd: &std::path::Path) {
    shell::with_local_node_bin_path(process, cwd);
}

pub(super) fn shell_quote(raw: &str) -> String {
    shell::shell_quote(raw)
}
