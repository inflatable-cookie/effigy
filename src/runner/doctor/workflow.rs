use super::checks::run_doctor_checks;
use super::progress::DoctorProgressReporter;
use super::report::DoctorReport;
use crate::runner::command_context::current_working_dir;
use crate::runner::error::RunnerError;
#[path = "workflow/handler.rs"]
mod handler;
#[path = "workflow/phases.rs"]
mod phases;
#[path = "workflow/preparation.rs"]
mod preparation;

pub(super) struct DoctorRunOutput {
    pub(super) report: DoctorReport,
    pub(super) error_count: usize,
}

pub(super) fn run_doctor_workflow(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
    progress: Option<&mut DoctorProgressReporter>,
) -> Result<DoctorRunOutput, RunnerError> {
    let cwd = current_working_dir()?;
    let mut handler = handler::DefaultWorkflowPhaseHandler::new(progress);
    phases::run_workflow_phase_pipeline(cwd, repo_override, fix, &mut handler)
}

#[cfg(test)]
mod tests;
