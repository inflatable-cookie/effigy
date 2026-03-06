use std::path::Path;

use crate::runner::doctor::report::DoctorSeverity;

use super::{check_id, remediation, SchemaContext};
use crate::runner::doctor::report::DoctorState;

#[test]
fn unsupported_manifest_root_finding_message_is_stable() {
    let mut state = DoctorState::new();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut state);

    context.unsupported_manifest_root();

    assert_eq!(state.findings.len(), 1);
    let finding = &state.findings[0];
    assert_eq!(finding.check_id, check_id::MANIFEST_PARSE);
    assert_eq!(finding.severity, DoctorSeverity::Error);
    assert_eq!(
        finding.evidence,
        "/tmp/effigy.toml root document must be a TOML table"
    );
    assert_eq!(finding.remediation, remediation::SCHEMA_TABLE_ROOT_REQUIRED);
}

#[test]
fn unsupported_key_finding_message_is_stable() {
    let mut state = DoctorState::new();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut state);

    context.unsupported_key("tasks.app.unknown");

    assert_eq!(state.findings.len(), 1);
    let finding = &state.findings[0];
    assert_eq!(finding.check_id, check_id::MANIFEST_SCHEMA_UNSUPPORTED_KEY);
    assert_eq!(finding.severity, DoctorSeverity::Error);
    assert_eq!(
        finding.evidence,
        "/tmp/effigy.toml contains unsupported key `tasks.app.unknown`"
    );
    assert_eq!(
        finding.remediation,
        remediation::SCHEMA_REMOVE_UNSUPPORTED_KEYS
    );
}

#[test]
fn unsupported_value_finding_message_is_stable() {
    let mut state = DoctorState::new();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut state);

    context.unsupported_value("tasks.api.run", "array", "expected string");

    assert_eq!(state.findings.len(), 1);
    let finding = &state.findings[0];
    assert_eq!(
        finding.check_id,
        check_id::MANIFEST_SCHEMA_UNSUPPORTED_VALUE
    );
    assert_eq!(finding.severity, DoctorSeverity::Error);
    assert_eq!(
        finding.evidence,
        "/tmp/effigy.toml has unsupported value at `tasks.api.run`: array"
    );
    assert_eq!(
        finding.remediation,
        "Use a supported value/type for `tasks.api.run` (expected string)."
    );
}
