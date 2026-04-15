use effigy_tasks::TaskRuntimeArgs;

use crate::runner::error::RunnerError;

pub(super) fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    match raw {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

pub(super) fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    effigy_tasks::parse_task_runtime_args(args).map_err(RunnerError::task_invocation)
}
