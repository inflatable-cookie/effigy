mod check_registry;
mod workflow;

use super::progress::DoctorProgressReporter;
use crate::runner::error::RunnerError;

pub(super) type DoctorRunOutput = workflow::DoctorRunOutput;

pub(super) fn run_doctor_workflow(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
    progress: Option<&mut DoctorProgressReporter>,
) -> Result<DoctorRunOutput, RunnerError> {
    workflow::run_doctor_workflow(repo_override, fix, progress)
}
