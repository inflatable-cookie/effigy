use std::path::Path;

use crate::TaskInvocation;

#[path = "deferral/run.rs"]
mod run;
#[path = "deferral/select.rs"]
mod select;
#[path = "deferral/trace.rs"]
mod trace;

use super::{DeferredCommand, LoadedCatalog, RunnerError, TaskRuntimeArgs, TaskSelector};

pub(super) fn should_attempt_deferral(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}

pub(super) fn select_deferral(
    selector: &TaskSelector,
    catalogs: &[LoadedCatalog],
    cwd: &Path,
    workspace_root: &Path,
) -> Option<DeferredCommand> {
    select::select_deferral(selector, catalogs, cwd, workspace_root)
}

pub(super) fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    run::run_deferred_request(task, runtime_args, deferral, cause)
}
