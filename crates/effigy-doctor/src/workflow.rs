use super::checks::run_doctor_checks;
use super::progress::DoctorProgressReporter;
use crate::util::current_working_dir;
use crate::{DoctorError, DoctorReport, DoctorRuntimePorts};
#[path = "workflow/handler.rs"]
mod handler;
#[path = "workflow/phases.rs"]
mod phases;
#[path = "workflow/preparation.rs"]
mod preparation;

pub(crate) struct DoctorRunOutput {
    pub(crate) report: DoctorReport,
    pub(crate) error_count: usize,
}

pub(crate) fn run_doctor_workflow(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
    progress: Option<&mut DoctorProgressReporter>,
    ports: &dyn DoctorRuntimePorts,
) -> Result<DoctorRunOutput, DoctorError> {
    let cwd = current_working_dir()?;
    let mut handler = handler::DefaultWorkflowPhaseHandler::new(progress, ports);
    phases::run_workflow_phase_pipeline(cwd, repo_override, fix, &mut handler)
}

#[cfg(test)]
mod tests;
