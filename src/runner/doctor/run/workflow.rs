use std::path::Path;

use crate::resolver::resolve_target_root;

use super::super::super::RunnerError;
use super::super::{
    finding_templates::WorkflowFinding, manifest, DoctorReport, DoctorState, ManifestSnapshot,
};
use super::check_registry::run_doctor_checks;
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
) -> Result<DoctorRunOutput, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let mut handler = DefaultWorkflowPhaseHandler;
    phases::run_workflow_phase_pipeline(cwd, repo_override, fix, &mut handler)
}

fn add_root_resolution_finding(
    resolved: &crate::resolver::ResolvedTarget,
    state: &mut DoctorState,
) {
    WorkflowFinding::RootResolution {
        resolved_root: &resolved.resolved_root,
        resolution_mode: resolved.resolution_mode,
    }
    .emit(state);
}

fn collect_manifest_snapshot(
    resolved_root: &Path,
    state: &mut DoctorState,
) -> Result<ManifestSnapshot, RunnerError> {
    let (manifest_paths, parsed_catalogs, preferred_js_pm, parse_ok_any) =
        manifest::collect_manifest_findings(resolved_root, state)?;
    Ok(ManifestSnapshot {
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    })
}

fn prepare_manifest_snapshot(
    resolved_root: &Path,
    should_fix: bool,
    state: &mut DoctorState,
) -> Result<ManifestSnapshot, RunnerError> {
    preparation::prepare_manifest_snapshot_with(
        resolved_root,
        should_fix,
        state,
        collect_manifest_snapshot,
        |root, snapshot, state| {
            state
                .fixes
                .extend(manifest::apply_fixers(root, &snapshot.parsed_catalogs));
        },
    )
}

fn add_manifest_availability_findings(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
) {
    if manifest.manifest_paths.is_empty() {
        WorkflowFinding::MissingManifestFiles { resolved_root }.emit(state);
    } else if !manifest.parse_ok_any {
        WorkflowFinding::NoValidManifests.emit(state);
    }
}

struct DefaultWorkflowPhaseHandler;

impl phases::WorkflowPhaseHandler for DefaultWorkflowPhaseHandler {
    fn resolve_root(
        &mut self,
        cwd: std::path::PathBuf,
        repo_override: Option<std::path::PathBuf>,
    ) -> Result<crate::resolver::ResolvedTarget, RunnerError> {
        resolve_target_root(cwd, repo_override).map_err(RunnerError::Resolve)
    }

    fn emit_root_resolution_finding(
        &mut self,
        resolved: &crate::resolver::ResolvedTarget,
        state: &mut DoctorState,
    ) {
        add_root_resolution_finding(resolved, state);
    }

    fn prepare_manifest(
        &mut self,
        resolved_root: &Path,
        fix: bool,
        state: &mut DoctorState,
    ) -> Result<ManifestSnapshot, RunnerError> {
        prepare_manifest_snapshot(resolved_root, fix, state)
    }

    fn run_checks(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    ) {
        run_doctor_checks(resolved_root, manifest, state);
    }

    fn finalize_fix_actions(&mut self, state: &mut DoctorState, fix: bool) {
        state.finalize_fix_actions(fix);
    }

    fn add_manifest_availability_findings(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    ) {
        add_manifest_availability_findings(resolved_root, manifest, state);
    }

    fn summarize_and_report(
        &mut self,
        state: DoctorState,
        resolved: crate::resolver::ResolvedTarget,
    ) -> DoctorRunOutput {
        let summary = state.summarize();
        let error_count = summary.error;
        let report = state.into_report(summary, resolved.evidence, resolved.warnings);
        DoctorRunOutput {
            report,
            error_count,
        }
    }
}

#[cfg(test)]
mod tests;
