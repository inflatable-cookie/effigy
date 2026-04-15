use std::path::Path;

use toml::Value;

use crate::{check_id, remediation, schema_supported_value, FindingSink};

pub(super) struct SchemaContext<'a, 'b> {
    manifest_path: &'a Path,
    sink: &'b mut dyn FindingSink,
}

impl<'a, 'b> SchemaContext<'a, 'b> {
    pub(super) fn new(manifest_path: &'a Path, sink: &'b mut dyn FindingSink) -> Self {
        Self {
            manifest_path,
            sink,
        }
    }

    pub(super) fn unsupported_manifest_root(&mut self) {
        self.sink.add_check_error(
            check_id::MANIFEST_PARSE,
            format!(
                "{} root document must be a TOML table",
                self.manifest_path.display()
            ),
            remediation::SCHEMA_TABLE_ROOT_REQUIRED.to_owned(),
        );
    }

    pub(super) fn unsupported_key(&mut self, key_path: &str) {
        self.sink.add_check_error(
            check_id::MANIFEST_SCHEMA_UNSUPPORTED_KEY,
            format!(
                "{} contains unsupported key `{}`",
                self.manifest_path.display(),
                key_path
            ),
            remediation::SCHEMA_REMOVE_UNSUPPORTED_KEYS.to_owned(),
        );
    }

    pub(super) fn unsupported_nested_key(&mut self, parent: &str, key: &str) {
        self.unsupported_key(&format!("{parent}.{key}"));
    }

    pub(super) fn unsupported_value(&mut self, key_path: &str, actual: &str, expected: &str) {
        self.sink.add_check_error(
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

    pub(super) fn invalid_value_type(&mut self, key_path: &str, expected: &str) {
        self.unsupported_value(key_path, "wrong value type", expected);
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
