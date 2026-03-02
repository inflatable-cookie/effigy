use std::path::Path;

use crate::resolver::resolve_target_root;

use super::super::RunnerError;
use super::super::TASK_MANIFEST_FILE;
use super::{
    conflicts, environment, health, manifest, references, DoctorFinding, DoctorReport,
    DoctorSeverity, DoctorState, ManifestSnapshot,
};

pub(super) struct DoctorRunOutput {
    pub(super) report: DoctorReport,
    pub(super) error_count: usize,
}

pub(super) fn run_doctor_workflow(
    repo_override: Option<std::path::PathBuf>,
    fix: bool,
) -> Result<DoctorRunOutput, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd, repo_override)?;

    let mut state = DoctorState::new();
    add_root_resolution_finding(&resolved, &mut state);

    let mut manifest = collect_manifest_snapshot(&resolved.resolved_root, &mut state)?;
    maybe_apply_manifest_fixes(fix, &resolved.resolved_root, &mut manifest, &mut state)?;
    run_doctor_checks(&resolved.resolved_root, &manifest, &mut state);
    state.finalize_fix_actions(fix);
    add_manifest_availability_findings(&resolved.resolved_root, &manifest, &mut state);

    let summary = state.summarize();
    let error_count = summary.error;
    let report = state.into_report(summary, resolved.evidence, resolved.warnings);
    Ok(DoctorRunOutput {
        report,
        error_count,
    })
}

fn add_root_resolution_finding(
    resolved: &crate::resolver::ResolvedTarget,
    state: &mut DoctorState,
) {
    let root_mode = match resolved.resolution_mode {
        crate::tasks::ResolutionMode::Explicit => "explicit (--repo)",
        crate::tasks::ResolutionMode::AutoNearest => "auto (nearest root)",
        crate::tasks::ResolutionMode::AutoPromoted => "auto (promoted workspace root)",
    };
    state.add_finding(DoctorFinding {
        check_id: "workspace.root-resolution".to_owned(),
        severity: DoctorSeverity::Info,
        evidence: format!(
            "resolved root `{}` using mode {root_mode}",
            resolved.resolved_root.display()
        ),
        remediation: "Use `--repo <PATH>` when you need deterministic root targeting.".to_owned(),
        fixable: false,
    });
}

fn collect_manifest_snapshot(
    resolved_root: &Path,
    state: &mut DoctorState,
) -> Result<ManifestSnapshot, RunnerError> {
    let (manifest_paths, parsed_catalogs, preferred_js_pm, parse_ok_any) =
        manifest::collect_manifest_findings(
            resolved_root,
            &mut state.findings,
            &mut state.statuses,
        )?;
    Ok(ManifestSnapshot {
        manifest_paths,
        parsed_catalogs,
        preferred_js_pm,
        parse_ok_any,
    })
}

fn maybe_apply_manifest_fixes(
    should_fix: bool,
    resolved_root: &Path,
    manifest: &mut ManifestSnapshot,
    state: &mut DoctorState,
) -> Result<(), RunnerError> {
    if !should_fix {
        return Ok(());
    }

    state.fixes.extend(manifest::apply_fixers(
        resolved_root,
        &manifest.parsed_catalogs,
    ));
    *manifest = collect_manifest_snapshot(resolved_root, state)?;
    Ok(())
}

fn run_doctor_checks(resolved_root: &Path, manifest: &ManifestSnapshot, state: &mut DoctorState) {
    conflicts::check_manifest_alias_conflicts(
        &manifest.parsed_catalogs,
        &mut state.findings,
        &mut state.statuses,
    );
    environment::check_environment_tools(
        resolved_root,
        &manifest.parsed_catalogs,
        manifest.preferred_js_pm,
        &mut state.findings,
        &mut state.statuses,
    );
    references::check_task_references(
        &manifest.parsed_catalogs,
        &mut state.findings,
        &mut state.statuses,
    );
    health::check_health_task(
        resolved_root,
        &manifest.parsed_catalogs,
        &mut state.findings,
        &mut state.statuses,
    );
}

fn add_manifest_availability_findings(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
) {
    if manifest.manifest_paths.is_empty() {
        state.add_finding(DoctorFinding {
            check_id: "manifest.parse".to_owned(),
            severity: DoctorSeverity::Warning,
            evidence: format!(
                "no `{}` files were discovered under {}",
                TASK_MANIFEST_FILE,
                resolved_root.display()
            ),
            remediation: "Add an `effigy.toml` at repo root or child catalog roots.".to_owned(),
            fixable: false,
        });
    } else if !manifest.parse_ok_any {
        state.add_finding(DoctorFinding {
            check_id: "manifest.parse".to_owned(),
            severity: DoctorSeverity::Error,
            evidence: "no valid manifests were available for downstream checks".to_owned(),
            remediation: "Fix manifest parse/schema errors first, then re-run `effigy doctor`."
                .to_owned(),
            fixable: false,
        });
    }
}
