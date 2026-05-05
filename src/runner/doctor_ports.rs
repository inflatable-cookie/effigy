//! Concrete [`DoctorRuntimePorts`] implementation for the runner.
//!
//! The trait itself lives in `effigy-doctor`. This module threads the
//! doctor orchestration layer's two reach-backs (health task
//! execution via `execute::run_manifest_task_with_cwd`; deferral
//! analysis via `deferral::select_deferral`) into the runner,
//! converting `RunnerError` to `DoctorError` at the port boundary so
//! the doctor layer only speaks its own error type.

use std::path::{Path, PathBuf};

use effigy_cli::TaskInvocation;
use effigy_doctor::{DoctorError, DoctorRuntimePorts};
use effigy_execution::ExecutionSurface;
use effigy_manifest::{DeferredCommand, LoadedCatalog};
use effigy_tasks::TaskSelector;

use crate::runner::deferral;
use crate::runner::error::RunnerError;
use crate::runner::execute::api;

#[derive(Debug, Default)]
pub(in crate::runner) struct RunnerDoctorPorts;

impl RunnerDoctorPorts {
    pub(in crate::runner) fn new() -> Self {
        Self
    }
}

impl DoctorRuntimePorts for RunnerDoctorPorts {
    fn run_manifest_task(
        &self,
        invocation: &TaskInvocation,
        cwd: PathBuf,
    ) -> Result<String, DoctorError> {
        api::run_manifest_task_with_surface(invocation, cwd, ExecutionSurface::DirectCli)
            .map_err(runner_to_doctor)
    }

    fn select_deferral(
        &self,
        selector: &TaskSelector,
        catalogs: &[LoadedCatalog],
        cwd: &Path,
        workspace_root: &Path,
    ) -> Option<DeferredCommand> {
        deferral::select_deferral(selector, catalogs, cwd, workspace_root)
    }
}

fn runner_to_doctor(error: RunnerError) -> DoctorError {
    match error {
        RunnerError::CommandJsonFailure { rendered } => {
            DoctorError::CommandJsonFailure { rendered }
        }
        RunnerError::TaskInvocation(message) => DoctorError::TaskInvocation(message),
        RunnerError::Ui(message) => DoctorError::Ui(message),
        other => DoctorError::TaskInvocation(other.to_string()),
    }
}
