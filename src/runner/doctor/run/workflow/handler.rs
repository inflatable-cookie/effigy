use std::path::Path;

use crate::resolver::{resolve_target_root, ResolvedTarget};

use super::super::super::report::{DoctorState, ManifestSnapshot};
use super::super::super::{finding_templates::WorkflowFinding, manifest};
use super::{phases, preparation, run_doctor_checks, DoctorProgressReporter, DoctorRunOutput};
use crate::runner::error::RunnerError;

pub(super) struct DefaultWorkflowPhaseHandler<'a> {
    progress: Option<&'a mut DoctorProgressReporter>,
}

impl<'a> DefaultWorkflowPhaseHandler<'a> {
    pub(super) fn new(progress: Option<&'a mut DoctorProgressReporter>) -> Self {
        Self { progress }
    }
}

impl phases::WorkflowPhaseHandler for DefaultWorkflowPhaseHandler<'_> {
    fn resolve_root(
        &mut self,
        cwd: std::path::PathBuf,
        repo_override: Option<std::path::PathBuf>,
    ) -> Result<ResolvedTarget, RunnerError> {
        resolve_target_root(cwd, repo_override).map_err(RunnerError::Resolve)
    }

    fn emit_root_resolution_finding(&mut self, resolved: &ResolvedTarget, state: &mut DoctorState) {
        emit_root_resolution_finding(resolved, state);
    }

    fn prepare_manifest(
        &mut self,
        resolved_root: &Path,
        fix: bool,
        state: &mut DoctorState,
    ) -> Result<ManifestSnapshot, RunnerError> {
        preparation::prepare_manifest_snapshot_with(
            resolved_root,
            fix,
            state,
            collect_manifest_snapshot,
            |root, snapshot, state| {
                state
                    .fixes
                    .extend(manifest::apply_fixers(root, &snapshot.parsed_catalogs));
            },
        )
    }

    fn run_checks(
        &mut self,
        resolved_root: &Path,
        manifest: &ManifestSnapshot,
        state: &mut DoctorState,
    ) {
        run_doctor_checks(resolved_root, manifest, state, self.progress.as_deref_mut());
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
        resolved: ResolvedTarget,
    ) -> DoctorRunOutput {
        summarize_and_report(state, resolved)
    }
}

pub(super) fn emit_root_resolution_finding(resolved: &ResolvedTarget, state: &mut DoctorState) {
    WorkflowFinding::RootResolution {
        resolved_root: &resolved.resolved_root,
        resolution_mode: resolved.resolution_mode,
    }
    .emit(state);
}

pub(super) fn add_manifest_availability_findings(
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

fn summarize_and_report(state: DoctorState, resolved: ResolvedTarget) -> DoctorRunOutput {
    let summary = state.summarize();
    let error_count = summary.error;
    let report = state.into_report(
        resolved.resolved_root.display().to_string(),
        summary,
        resolved.evidence,
        resolved.warnings,
    );
    DoctorRunOutput {
        report,
        error_count,
    }
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
