use std::path::{Path, PathBuf};

use crate::resolver::ResolvedTarget;
use effigy_tasks::ResolutionMode;

use super::super::super::super::model::constants::TASK_MANIFEST_FILE;
use super::super::super::contracts::{check_id, remediation};
use super::super::super::report::{DoctorSeverity, DoctorState, ManifestSnapshot};
use super::*;

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
