use super::super::progress::DoctorProgressReporter;
use super::super::report::DoctorReport;
use super::check_registry::run_doctor_checks;
use crate::runner::command_context::current_working_dir;
use crate::runner::error::RunnerError;
#[path = "workflow/handler.rs"]
mod handler;
#[path = "workflow/phases.rs"]
mod phases;
#[path = "workflow/preparation.rs"]
mod preparation;

pub(in super::super) struct DoctorRunOutput {
    pub(in super::super) report: DoctorReport,
    pub(in super::super) error_count: usize,
}

pub(in super::super) fn run_doctor_workflow(
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
