use std::path::{Path, PathBuf};

use crate::resolver::ResolvedTarget;

use super::super::super::report::{DoctorState, ManifestSnapshot};
use super::DoctorRunOutput;
use crate::runner::error::RunnerError;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkflowPhase {
    RootResolution,
    ManifestPreparation,
    Checks,
    FixFinalization,
    ManifestAvailabilityFindings,
    SummaryAndReport,
}

#[cfg(test)]
const WORKFLOW_PHASE_ORDER: [WorkflowPhase; 6] = [
    WorkflowPhase::RootResolution,
    WorkflowPhase::ManifestPreparation,
    WorkflowPhase::Checks,
    WorkflowPhase::FixFinalization,
    WorkflowPhase::ManifestAvailabilityFindings,
    WorkflowPhase::SummaryAndReport,
];

#[cfg(test)]
pub(super) fn workflow_phase_order() -> &'static [WorkflowPhase] {
    &WORKFLOW_PHASE_ORDER
}

pub(super) trait WorkflowPhaseHandler {
    fn resolve_root(
        &mut self,
        cwd: PathBuf,
        repo_override: Option<PathBuf>,
    ) -> Result<ResolvedTarget, RunnerError>;

    fn emit_root_resolution_finding(&mut self, resolved: &ResolvedTarget, state: &mut DoctorState);

    fn prepare_manifest(
        &mut self,
        resolved_root: &Path,
        fix: bool,
        state: &mut DoctorState,
    ) -> Result<ManifestSnapshot, RunnerError>;

    fn run_checks(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    );

    fn finalize_fix_actions(&mut self, state: &mut DoctorState, fix: bool);

    fn add_manifest_availability_findings(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    );

    fn summarize_and_report(
        &mut self,
        state: DoctorState,
        resolved: ResolvedTarget,
    ) -> DoctorRunOutput;
}

pub(super) fn run_workflow_phase_pipeline<H: WorkflowPhaseHandler>(
    cwd: PathBuf,
    repo_override: Option<PathBuf>,
    fix: bool,
    handler: &mut H,
) -> Result<DoctorRunOutput, RunnerError> {
    let mut state = DoctorState::new();
    let resolved = handler.resolve_root(cwd, repo_override)?;
    handler.emit_root_resolution_finding(&resolved, &mut state);

    let manifest = handler.prepare_manifest(&resolved.resolved_root, fix, &mut state)?;
    handler.run_checks(&resolved.resolved_root, &manifest, &mut state);
    handler.finalize_fix_actions(&mut state, fix);
    handler.add_manifest_availability_findings(&resolved.resolved_root, &manifest, &mut state);

    Ok(handler.summarize_and_report(state, resolved))
}

#[cfg(test)]
#[path = "phases/tests.rs"]
mod tests;
