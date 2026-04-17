//! Thin adapter — the parser lives in `effigy-tasks::reference`.
//!
//! Moved out in batch 241. Wraps the shared `Result<_, String>`
//! return channel back into `RunnerError::task_invocation` so runner
//! callers see the same error shape they had before.

use effigy_tasks::TaskSelector;

use crate::runner::error::RunnerError;

pub(super) fn parse_task_reference_invocation(
    raw: &str,
) -> Result<(TaskSelector, Vec<String>), RunnerError> {
    effigy_tasks::parse_task_reference_invocation(raw).map_err(RunnerError::task_invocation)
}

pub(super) fn render_passthrough_args(args: &[String]) -> String {
    effigy_tasks::render_passthrough_args(args)
}
