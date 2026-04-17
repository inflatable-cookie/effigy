use effigy_cli::TaskInvocation;

use super::super::super::super::deferral::{
    run_deferred_request, select_deferral, should_attempt_deferral,
};
use super::super::super::preflight::ExecutionPreflight;
use crate::runner::error::RunnerError;

pub(super) fn resolve_deferred_selection_output(
    task: &TaskInvocation,
    preflight: &ExecutionPreflight,
    selection_error: &RunnerError,
) -> Result<Option<String>, RunnerError> {
    if !should_attempt_deferral(selection_error) {
        return Ok(None);
    }
    let Some(deferral) = select_deferral(
        &preflight.selector,
        &preflight.catalogs,
        &preflight.invocation_cwd,
        &preflight.resolved.resolved_root,
    ) else {
        return Ok(None);
    };
    run_deferred_request(
        task,
        &preflight.runtime_args_raw,
        &deferral,
        selection_error,
    )
    .map(Some)
}
