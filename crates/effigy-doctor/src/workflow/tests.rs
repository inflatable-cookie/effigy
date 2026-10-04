use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_cli::TaskInvocation;
use effigy_core::resolver::ResolvedTarget;
use effigy_manifest::{DeferredCommand, LoadedCatalog};
use effigy_tasks::{ResolutionMode, TaskSelector};

use super::*;
use crate::contracts::{check_id, remediation};
use crate::{
    manifest_snapshot::ManifestSnapshot, DoctorRuntimeDiagnostics, DoctorRuntimePorts,
    DoctorSeverity, DoctorState,
};
use effigy_manifest::TASK_MANIFEST_FILE;

fn empty_manifest_snapshot() -> ManifestSnapshot {
    ManifestSnapshot {
        manifest_paths: Vec::new(),
        parsed_catalogs: Vec::new(),
        preferred_js_pm: None,
        parse_ok_any: false,
    }
}

#[test]
fn manifest_availability_missing_manifest_message_is_stable() {
    let mut state = DoctorState::new();
    let manifest = empty_manifest_snapshot();
    let root = Path::new("/tmp/doctor-workspace");

    handler::add_manifest_availability_findings(root, &manifest, &mut state);

    assert_eq!(state.findings.len(), 1);
    let finding = &state.findings[0];
    assert_eq!(finding.check_id, check_id::MANIFEST_PARSE);
    assert_eq!(finding.severity, DoctorSeverity::Warning);
    assert_eq!(
        finding.evidence,
        format!(
            "no `{}` files were discovered under {}",
            TASK_MANIFEST_FILE,
            root.display()
        )
    );
    assert_eq!(finding.remediation, remediation::ADD_MANIFEST);
}

#[test]
fn manifest_availability_parse_failure_message_is_stable() {
    let mut state = DoctorState::new();
    let manifest = ManifestSnapshot {
        manifest_paths: vec![PathBuf::from("/tmp/doctor-workspace/effigy.toml")],
        parsed_catalogs: Vec::new(),
        preferred_js_pm: None,
        parse_ok_any: false,
    };

    handler::add_manifest_availability_findings(
        Path::new("/tmp/doctor-workspace"),
        &manifest,
        &mut state,
    );

    assert_eq!(state.findings.len(), 1);
    let finding = &state.findings[0];
    assert_eq!(finding.check_id, check_id::MANIFEST_PARSE);
    assert_eq!(finding.severity, DoctorSeverity::Error);
    assert_eq!(
        finding.evidence,
        "no valid manifests were available for downstream checks"
    );
    assert_eq!(finding.remediation, remediation::FIX_MANIFEST_ERRORS_FIRST);
}

#[test]
fn root_resolution_finding_tracks_expected_mode_labels_and_contract() {
    let cases = [
        (ResolutionMode::Explicit, "explicit (--repo)"),
        (ResolutionMode::AutoNearest, "auto (nearest root)"),
        (
            ResolutionMode::AutoPromoted,
            "auto (promoted workspace root)",
        ),
    ];

    for (mode, mode_label) in cases {
        let mut state = DoctorState::new();
        let resolved = ResolvedTarget {
            resolved_root: PathBuf::from("/tmp/doctor-workspace"),
            resolution_mode: mode,
            evidence: Vec::new(),
            warnings: Vec::new(),
        };
        handler::emit_root_resolution_finding(&resolved, &mut state);

        assert_eq!(state.findings.len(), 1);
        let finding = &state.findings[0];
        assert_eq!(finding.check_id, check_id::WORKSPACE_ROOT_RESOLUTION);
        assert_eq!(finding.severity, DoctorSeverity::Info);
        assert!(finding.evidence.contains(mode_label));
        assert_eq!(finding.remediation, remediation::USE_REPO_OVERRIDE);
    }
}

#[test]
fn runtime_diagnostic_findings_are_included_before_summary() {
    let state = DoctorState::new();
    let resolved = ResolvedTarget {
        resolved_root: PathBuf::from("/tmp/doctor-workspace"),
        resolution_mode: ResolutionMode::Explicit,
        evidence: Vec::new(),
        warnings: Vec::new(),
    };
    let diagnostics = crate::DoctorRuntimeDiagnostics {
        evidence: Vec::new(),
        warnings: Vec::new(),
        findings: vec![crate::DoctorFinding {
            check_id: check_id::CONTAINER_WORKSPACE_OWNERSHIP.to_owned(),
            severity: DoctorSeverity::Warning,
            evidence: "root-owned cache path".to_owned(),
            remediation: "repair ownership".to_owned(),
            fixable: false,
        }],
    };

    let output = handler::summarize_and_report_with_diagnostics(state, resolved, diagnostics);

    assert_eq!(output.report.summary.warning, 1);
    assert_eq!(output.report.summary.checks, crate::ALL_CHECK_IDS.len());
    assert_eq!(output.report.findings.len(), 1);
    assert_eq!(
        output.report.findings[0].check_id,
        check_id::CONTAINER_WORKSPACE_OWNERSHIP
    );
}

#[test]
fn runtime_diagnostics_run_when_budget_already_expired() {
    struct RecordingPorts {
        remaining: Cell<Option<Option<Duration>>>,
    }

    impl DoctorRuntimePorts for RecordingPorts {
        fn run_manifest_task(
            &self,
            _invocation: &TaskInvocation,
            _cwd: PathBuf,
        ) -> Result<String, crate::DoctorError> {
            Ok(String::new())
        }

        fn select_deferral(
            &self,
            _selector: &TaskSelector,
            _catalogs: &[LoadedCatalog],
            _cwd: &Path,
            _workspace_root: &Path,
        ) -> Option<DeferredCommand> {
            None
        }

        fn runtime_diagnostics(
            &self,
            _resolved_root: &Path,
        ) -> Result<DoctorRuntimeDiagnostics, crate::DoctorError> {
            panic!("unbounded runtime_diagnostics must not be used once a budget exists");
        }

        fn runtime_diagnostics_bounded(
            &self,
            _resolved_root: &Path,
            remaining_budget: Option<Duration>,
        ) -> Result<DoctorRuntimeDiagnostics, crate::DoctorError> {
            self.remaining.set(Some(remaining_budget));
            Ok(DoctorRuntimeDiagnostics {
                evidence: Vec::new(),
                warnings: vec!["workspace ownership probe skipped: timed out".to_owned()],
                findings: Vec::new(),
            })
        }
    }

    let ports = RecordingPorts {
        remaining: Cell::new(None),
    };
    let config = DoctorRunConfig {
        mode: crate::DoctorMode::Fast,
        catalog: None,
        all_catalogs: false,
        refresh: false,
        budget: Some(Duration::ZERO),
    };
    let mut handler = handler::DefaultWorkflowPhaseHandler::new(
        PathBuf::from("/tmp/doctor-workspace"),
        config,
        None,
        &ports,
    );
    let output = phases::WorkflowPhaseHandler::summarize_and_report(
        &mut handler,
        DoctorState::new(),
        ResolvedTarget {
            resolved_root: PathBuf::from("/tmp/doctor-workspace"),
            resolution_mode: ResolutionMode::Explicit,
            evidence: Vec::new(),
            warnings: Vec::new(),
        },
    );

    assert_eq!(ports.remaining.get(), Some(Some(Duration::ZERO)));
    assert!(
        output
            .report
            .root_warnings
            .iter()
            .any(|warning| warning.contains("timed out")),
        "expired budget must still emit an unavailable warning, got {:?}",
        output.report.root_warnings
    );
    assert_eq!(
        output.report.timeout_phase.as_deref(),
        Some("runtime_diagnostics")
    );
    assert!(!output.report.complete);
}
