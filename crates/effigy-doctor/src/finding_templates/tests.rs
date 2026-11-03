use std::io::Error;
use std::path::Path;

use super::*;
use crate::contracts::{check_id, remediation};
use crate::{DoctorSeverity, DoctorState};

// The workflow root-resolution and manifest-availability finding contracts are
// asserted through their real dispatch paths in `workflow::tests`
// (`root_resolution_finding_tracks_expected_mode_labels_and_contract`,
// `manifest_availability_missing_manifest_message_is_stable`,
// `manifest_availability_parse_failure_message_is_stable`), which drive the
// same `WorkflowFinding` templates via `handler::{
// emit_root_resolution_finding,add_manifest_availability_findings}`; the
// bootstrap template contract lives with its integration survivor in
// `health::tests::missing_child_bootstrap_emits_finding_and_skips_health`.

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
