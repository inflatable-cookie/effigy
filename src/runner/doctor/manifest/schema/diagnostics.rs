use std::path::Path;

use toml::Value;

use super::super::super::contracts::{check_id, remediation, schema_supported_value};
use super::super::super::DoctorState;

pub(super) struct SchemaContext<'a, 'b> {
    manifest_path: &'a Path,
    state: &'b mut DoctorState,
}

impl<'a, 'b> SchemaContext<'a, 'b> {
    pub(super) fn new(manifest_path: &'a Path, state: &'b mut DoctorState) -> Self {
        Self {
            manifest_path,
            state,
        }
    }

    pub(super) fn unsupported_manifest_root(&mut self) {
        self.state.add_check_error(
            check_id::MANIFEST_PARSE,
            format!(
                "{} root document must be a TOML table",
                self.manifest_path.display()
            ),
            remediation::SCHEMA_TABLE_ROOT_REQUIRED,
        );
    }

    pub(super) fn unsupported_key(&mut self, key_path: &str) {
        self.state.add_check_error(
            check_id::MANIFEST_SCHEMA_UNSUPPORTED_KEY,
            format!(
                "{} contains unsupported key `{}`",
                self.manifest_path.display(),
                key_path
            ),
            remediation::SCHEMA_REMOVE_UNSUPPORTED_KEYS,
        );
    }

    pub(super) fn unsupported_value(&mut self, key_path: &str, actual: &str, expected: &str) {
        self.state.add_check_error(
            check_id::MANIFEST_SCHEMA_UNSUPPORTED_VALUE,
            format!(
                "{} has unsupported value at `{}`: {}",
                self.manifest_path.display(),
                key_path,
                actual
            ),
            schema_supported_value(key_path, expected),
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

#[cfg(test)]
#[path = "diagnostics/tests.rs"]
mod tests;
