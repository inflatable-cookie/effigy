use std::path::Path;

use super::{check_id, remediation, SchemaContext};
use crate::{DoctorFinding, DoctorSeverity, FindingSink};

#[derive(Default)]
struct TestSink {
    findings: Vec<DoctorFinding>,
}

impl FindingSink for TestSink {
    fn add_check_error(&mut self, check_id: &str, evidence: String, remediation: String) {
        self.findings.push(DoctorFinding {
            check_id: check_id.to_owned(),
            severity: DoctorSeverity::Error,
            evidence,
            remediation,
            fixable: false,
        });
    }
}

#[test]
fn unsupported_manifest_root_finding_message_is_stable() {
    let mut sink = TestSink::default();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut sink);

    context.unsupported_manifest_root();

    assert_eq!(sink.findings.len(), 1);
    let finding = &sink.findings[0];
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
    let mut sink = TestSink::default();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut sink);

    context.unsupported_key("tasks.app.unknown");

    assert_eq!(sink.findings.len(), 1);
    let finding = &sink.findings[0];
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
    let mut sink = TestSink::default();
    let mut context = SchemaContext::new(Path::new("/tmp/effigy.toml"), &mut sink);

    context.unsupported_value("tasks.api.run", "array", "expected string");

    assert_eq!(sink.findings.len(), 1);
    let finding = &sink.findings[0];
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
