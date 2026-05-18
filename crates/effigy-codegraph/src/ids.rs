use serde::{Deserialize, Serialize};

use crate::error::CodeGraphError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GraphId(String);

impl GraphId {
    pub fn new(value: impl Into<String>) -> Result<Self, CodeGraphError> {
        let value = value.into();
        validate_token("graph id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for GraphId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ExtractorId(String);

impl ExtractorId {
    pub fn new(value: impl Into<String>) -> Result<Self, CodeGraphError> {
        let value = value.into();
        validate_token("extractor id", &value)?;
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ExtractorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub(crate) fn validate_token(label: &str, value: &str) -> Result<(), CodeGraphError> {
    if value.trim().is_empty() {
        return Err(CodeGraphError::validation(format!(
            "{label} must not be empty"
        )));
    }
    if value.chars().any(char::is_whitespace) {
        return Err(CodeGraphError::validation(format!(
            "{label} must not contain whitespace"
        )));
    }
    if value.chars().any(char::is_control) {
        return Err(CodeGraphError::validation(format!(
            "{label} must not contain control characters"
        )));
    }
    Ok(())
}
