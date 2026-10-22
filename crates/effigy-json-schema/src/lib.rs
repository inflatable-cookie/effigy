//! The one JSON Schema validation engine in Effigy.
//!
//! `effigy contracts check-json` validates a project's declared selection
//! contract through this module, and the planned Rhai JSON Schema API
//! (Draft 2020-12) calls the same functions instead of adding a second
//! validator. The crate has no Effigy dependencies, so `effigy-rhai` can depend
//! on it without pulling in a command surface.
//!
//! # Draft and keywords
//!
//! Schemas are compiled as Draft 2020-12 documents regardless of the `$schema`
//! value. The engine is the maintained [`jsonschema`] crate with default
//! features off: the core, applicator, unevaluated, validation and meta-data
//! vocabularies are supported, including `$ref`, `$defs`, `$anchor` and
//! `$dynamicRef` inside the supplied document.
//!
//! # Unsupported and annotative behaviour
//!
//! These are deliberate limits, so a caller never mistakes an annotation for a
//! proof:
//!
//! - `format` is annotation only. Effigy does not enable format assertions, so
//!   `format: "uri"` or `format: "date"` never fails a document on its own.
//!   Validate those values in the producing code.
//! - `contentEncoding` and `contentMediaType` are annotations. Effigy does not
//!   decode or inspect embedded content.
//! - `$ref`/`$dynamicRef` resolve inside the document that is passed in. Remote
//!   retrieval (`http`, `file`) is not configured: a schema whose references
//!   leave the document is rejected when the schema is compiled.
//! - A schema that is not itself a valid Draft 2020-12 document is reported as
//!   an error, never as a passing instance.
//!
//! Errors are returned as `"<json pointer>: <message>"` lines, sorted and
//! de-duplicated so a report is stable between runs.

use serde_json::Value;

/// A schema or contract the engine refused to accept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    pub message: String,
}

impl SchemaError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for SchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for SchemaError {}

/// A Draft 2020-12 schema compiled once and reusable for many instances.
///
/// Compilation is where an invalid schema or an unreachable `$ref` surfaces.
#[derive(Debug)]
pub struct CompiledSchema {
    validator: jsonschema::Validator,
}

impl CompiledSchema {
    /// Compile `schema` as Draft 2020-12.
    pub fn compile(schema: &Value) -> Result<Self, SchemaError> {
        jsonschema::draft202012::new(schema)
            .map(|validator| Self { validator })
            .map_err(|error| SchemaError::new(format!("invalid JSON Schema: {error}")))
    }

    /// Validate `instance`, returning every failure as a stable message.
    pub fn validate(&self, instance: &Value) -> Vec<String> {
        let mut errors: Vec<String> = self
            .validator
            .iter_errors(instance)
            .map(|error| {
                let path = error.instance_path().as_str();
                if path.is_empty() {
                    format!("<root>: {error}")
                } else {
                    format!("{path}: {error}")
                }
            })
            .collect();
        errors.sort();
        errors.dedup();
        errors
    }
}

/// Compile `schema` and validate `instance` in one step.
pub fn validate(schema: &Value, instance: &Value) -> Result<Vec<String>, SchemaError> {
    CompiledSchema::compile(schema).map(|compiled| compiled.validate(instance))
}

#[cfg(test)]
mod tests;
