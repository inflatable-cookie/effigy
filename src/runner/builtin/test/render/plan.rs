use std::collections::BTreeSet;
use std::path::Path;

#[path = "plan_payload.rs"]
mod plan_payload;
#[path = "plan_projection.rs"]
mod plan_projection;
#[path = "plan_text.rs"]
mod plan_text;

use crate::TaskInvocation;

use super::super::execution::should_run_builtin_test_tui;
use super::super::planning::{BuiltinTestCliFlags, BuiltinTestTarget};
use super::super::suite_selection::BuiltinSuiteSelectionError;
use super::super::RunnerError;

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
    Err(RunnerError::TaskInvocation(selection_error.message))
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

    if flags.output_json {
        let payload = build_builtin_test_plan_payload(
            task,
            root,
            targets,
            requested_suite,
            passthrough,
            runtime_mode,
        );
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }

    render_builtin_test_plan_text(
        task,
        root,
        targets,
        requested_suite,
        passthrough,
        runnable_count,
        runtime_mode,
    )
}

fn render_builtin_test_plan_recovery(
    task: &TaskInvocation,
    root: &Path,
    available_runners: &BTreeSet<String>,
    message: &str,
    output_json: bool,
) -> Result<String, RunnerError> {
    if output_json {
        let payload =
            build_builtin_test_plan_recovery_payload(task, root, available_runners, message);
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }
    render_builtin_test_plan_recovery_text(task, root, available_runners, message)
}
