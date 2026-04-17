use std::collections::BTreeSet;
use std::path::Path;

#[path = "plan_payload.rs"]
mod plan_payload;
#[path = "plan_projection.rs"]
mod plan_projection;
#[path = "plan_text.rs"]
mod plan_text;

use effigy_cli::TaskInvocation;

use super::super::super::response::render_text_or_json_lazy;
use super::super::execution::should_run_builtin_test_tui;
use super::super::planning::{BuiltinTestCliFlags, BuiltinTestTarget};
use super::super::suite_selection::BuiltinSuiteSelectionError;
use crate::runner::error::RunnerError;

use plan_payload::{build_builtin_test_plan_payload, build_builtin_test_plan_recovery_payload};
use plan_text::{render_builtin_test_plan_recovery_text, render_builtin_test_plan_text};

pub(crate) fn render_suite_selection_failure(
    task: &TaskInvocation,
    resolved_root: &Path,
    flags: BuiltinTestCliFlags,
    selection_error: BuiltinSuiteSelectionError,
) -> Result<Option<String>, RunnerError> {
    if flags.plan_mode {
        return render_builtin_test_plan_recovery(
            task,
            resolved_root,
            &selection_error.available_runners,
            &selection_error.message,
            flags.output_json,
        )
        .map(Some);
    }
    Err(RunnerError::task_invocation(selection_error.message))
}

pub(crate) fn render_builtin_test_plan(
    task: &TaskInvocation,
    root: &Path,
    targets: &[BuiltinTestTarget],
    requested_suite: Option<&str>,
    passthrough: &[String],
    runnable_count: usize,
    flags: BuiltinTestCliFlags,
) -> Result<String, RunnerError> {
    let runtime_mode = if should_run_builtin_test_tui(flags.tui, runnable_count) {
        "tui"
    } else {
        "text"
    };
    render_text_or_json_lazy(
        flags.output_json,
        || {
            render_builtin_test_plan_text(
                task,
                root,
                targets,
                requested_suite,
                passthrough,
                runnable_count,
                runtime_mode,
            )
        },
        || {
            build_builtin_test_plan_payload(
                task,
                root,
                targets,
                requested_suite,
                passthrough,
                runtime_mode,
            )
        },
    )
}

fn render_builtin_test_plan_recovery(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    render_text_or_json_lazy(
        output_json,
        || render_builtin_test_plan_recovery_text(task, root, available_runners, message),
        || build_builtin_test_plan_recovery_payload(task, root, available_runners, message),
    )
}
