use std::io::Error;
use std::path::Path;

use crate::runner::doctor::report::DoctorSeverity;
use crate::tasks::ResolutionMode;

use super::super::contracts::{check_id, remediation};
use super::super::report::DoctorState;
use super::*;

#[test]
fn workflow_root_resolution_message_contract_is_stable() {
    let cases = [
        (ResolutionMode::Explicit, "explicit (--repo)"),
        (ResolutionMode::AutoNearest, "auto (nearest root)"),
        (
            ResolutionMode::AutoPromoted,
            "auto (promoted workspace root)",
        ),
    ];

    for (mode, label) in cases {
        let mut state = DoctorState::new();
        WorkflowFinding::RootResolution {
            resolved_root: Path::new("/tmp/workspace"),
            resolution_mode: mode,
        }
        .emit(&mut state);

        assert_eq!(state.findings.len(), 1);
        let finding = &state.findings[0];
        assert_eq!(finding.check_id, check_id::WORKSPACE_ROOT_RESOLUTION);
        assert_eq!(finding.severity, DoctorSeverity::Info);
        assert!(finding.evidence.contains(label));
        assert_eq!(finding.remediation, remediation::USE_REPO_OVERRIDE);
    }
}

#[test]
fn workflow_manifest_availability_contracts_are_stable() {
    let mut state = DoctorState::new();
    WorkflowFinding::MissingManifestFiles {
        resolved_root: Path::new("/tmp/workspace"),
    }
    .emit(&mut state);
    WorkflowFinding::NoValidManifests.emit(&mut state);

    assert_eq!(state.findings.len(), 2);
    assert_eq!(state.findings[0].check_id, check_id::MANIFEST_PARSE);
    assert_eq!(state.findings[0].severity, DoctorSeverity::Warning);
    assert_eq!(state.findings[0].remediation, remediation::ADD_MANIFEST);
    assert_eq!(state.findings[1].check_id, check_id::MANIFEST_PARSE);
    assert_eq!(state.findings[1].severity, DoctorSeverity::Error);
    assert_eq!(
        state.findings[1].evidence,
        "no valid manifests were available for downstream checks"
    );
    assert_eq!(
        state.findings[1].remediation,
        remediation::FIX_MANIFEST_ERRORS_FIRST
    );
}

#[test]
fn manifest_parse_templates_preserve_message_contract() {
    let path = Path::new("/tmp/workspace/effigy.toml");
    let mut state = DoctorState::new();
    ManifestParseFinding::read_failure(path, Error::other("read failure")).emit(&mut state);
    ManifestParseFinding::toml_syntax_failure(path, "syntax failure").emit(&mut state);
    ManifestParseFinding::strict_parse_failure(path, "strict failure").emit(&mut state);

    assert_eq!(state.findings.len(), 3);
    assert_eq!(state.findings[0].check_id, check_id::MANIFEST_PARSE);
    assert_eq!(
        state.findings[0].evidence,
        "failed to read /tmp/workspace/effigy.toml: read failure"
    );
    assert_eq!(
        state.findings[0].remediation,
        remediation::MANIFEST_READ_FAILURE
    );
    assert_eq!(
        state.findings[1].evidence,
        "failed to parse TOML syntax in /tmp/workspace/effigy.toml: syntax failure"
    );
    assert_eq!(
        state.findings[1].remediation,
        remediation::MANIFEST_TOML_SYNTAX
    );
    assert_eq!(
        state.findings[2].evidence,
        "strict manifest parse failed in /tmp/workspace/effigy.toml: strict failure"
    );
    assert_eq!(
        state.findings[2].remediation,
        remediation::MANIFEST_STRICT_PARSE
    );
}

#[test]
fn health_templates_preserve_message_contract() {
    let mut state = DoctorState::new();
    HealthFinding::discovery_missing().emit(&mut state);
    HealthFinding::discovery_found(&["root".to_owned(), "api".to_owned()]).emit(&mut state);
    HealthFinding::execution_success("health task executed successfully".to_owned())
        .emit(&mut state);
    HealthFinding::execution_failure("health task execution failed: exit=1".to_owned())
        .emit(&mut state);

    assert_eq!(state.findings.len(), 4);
    assert_eq!(state.findings[0].check_id, check_id::HEALTH_TASK_DISCOVERY);
    assert_eq!(state.findings[0].severity, DoctorSeverity::Warning);
    assert!(state.findings[0].fixable);
    assert_eq!(
        state.findings[0].remediation,
        remediation::DEFINE_HEALTH_TASK
    );
    assert_eq!(
        state.findings[1].evidence,
        "discovered `health` task in: root, api"
    );
    assert_eq!(state.findings[1].severity, DoctorSeverity::Info);
    assert_eq!(
        state.findings[2].evidence,
        "health task executed successfully"
    );
    assert_eq!(state.findings[2].check_id, check_id::HEALTH_TASK_EXECUTE);
    assert_eq!(
        state.findings[3].evidence,
        "health task execution failed: exit=1"
    );
    assert_eq!(state.findings[3].severity, DoctorSeverity::Error);
}
