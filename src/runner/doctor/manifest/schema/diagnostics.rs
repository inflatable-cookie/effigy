use std::collections::HashMap;
use std::path::Path;

use toml::Value;

use super::super::super::{DoctorFinding, DoctorSeverity};

pub(super) struct SchemaContext<'a, 'b> {
    manifest_path: &'a Path,
    findings: &'b mut Vec<DoctorFinding>,
    statuses: &'b mut HashMap<String, DoctorSeverity>,
}

impl<'a, 'b> SchemaContext<'a, 'b> {
    pub(super) fn new(
        manifest_path: &'a Path,
        findings: &'b mut Vec<DoctorFinding>,
        statuses: &'b mut HashMap<String, DoctorSeverity>,
    ) -> Self {
        Self {
            manifest_path,
            findings,
            statuses,
        }
    }

    pub(super) fn unsupported_manifest_root(&mut self) {
        super::super::super::add_finding(
            self.findings,
            self.statuses,
            DoctorFinding {
                check_id: "manifest.parse".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} root document must be a TOML table",
                    self.manifest_path.display()
                ),
                remediation: "Use table-based TOML with sections like `[tasks]`.".to_owned(),
                fixable: false,
            },
        );
    }

    pub(super) fn unsupported_key(&mut self, key_path: &str) {
        super::super::super::add_finding(
            self.findings,
            self.statuses,
            DoctorFinding {
                check_id: "manifest.schema.unsupported_key".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} contains unsupported key `{}`",
                    self.manifest_path.display(),
                    key_path
                ),
                remediation: "Remove/rename unsupported keys to match `effigy config --schema`."
                    .to_owned(),
                fixable: false,
            },
        );
    }

    pub(super) fn unsupported_value(&mut self, key_path: &str, actual: &str, expected: &str) {
        super::super::super::add_finding(
            self.findings,
            self.statuses,
            DoctorFinding {
                check_id: "manifest.schema.unsupported_value".to_owned(),
                severity: DoctorSeverity::Error,
                evidence: format!(
                    "{} has unsupported value at `{}`: {}",
                    self.manifest_path.display(),
                    key_path,
                    actual
                ),
                remediation: format!("Use a supported value/type for `{key_path}` ({expected})."),
                fixable: false,
            },
        );
    }

    pub(super) fn value_type(value: &Value) -> &'static str {
        match value {
            Value::String(_) => "string",
            Value::Integer(_) => "integer",
            Value::Float(_) => "float",
            Value::Boolean(_) => "boolean",
            Value::Datetime(_) => "datetime",
            Value::Array(_) => "array",
            Value::Table(_) => "table",
        }
    }
}
