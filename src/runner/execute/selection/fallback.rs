#[path = "fallback/builtin.rs"]
mod builtin;
#[path = "fallback/deferred.rs"]
mod deferred;

use effigy_cli::TaskInvocation;

use super::super::preflight::ExecutionPreflight;
use super::super::selection::result;
use super::super::selection::SelectionResolution;
use crate::runner::error::RunnerError;

pub(super) fn resolve_selection_error<'a>(
    task: &TaskInvocation,
    preflight: &'a ExecutionPreflight,
    error: RunnerError,
) -> Result<SelectionResolution<'a>, RunnerError> {
    if let Some(removed_builtin_error) =
        builtin::removed_builtin_invocation_error(&preflight.selector)
    {
        return Err(removed_builtin_error);
    }
    if let Some(output) = builtin::resolve_builtin_selection_output(task, preflight)? {
        return Ok(result::output(output));
    }
    if let Some(output) = deferred::resolve_deferred_selection_output(task, preflight, &error)? {
        return Ok(result::output(output));
    }
    Err(error)
}
