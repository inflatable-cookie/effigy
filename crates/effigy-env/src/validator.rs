use regex::Regex;

use super::ast::{EntryAnnotations, EnvSchema, EnvType};
use super::error::ValidationError;
use super::resolver::ResolvedEnv;

/// Validate resolved values against their declared types.
///
/// Returns a list of validation errors (empty if all values are valid).
pub fn validate_resolved_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for entry in &schema.entries {
        let validators = collect_validators(&entry.annotations);
        if validators.is_empty() {
            continue;
        }
        let Some(value) = resolved.get_value(&entry.key) else {
            continue;
        };

        for validator in validators {
            if let Err(expected) = validator.validate(value) {
                errors.push(ValidationError::new(
                    entry.key.clone(),
                    expected,
                    value,
                    entry.line,
                    entry.annotations.sensitive,
                ));
            }
        }
    }

    errors
}

trait Validator {
    fn validate(&self, value: &str) -> Result<(), String>;
}

struct PortValidator;

impl Validator for PortValidator {
    fn validate(&self, value: &str) -> Result<(), String> {
        let n: u16 = value
            .parse()
            .map_err(|_| "port (integer 1-65535)".to_owned())?;
        if n == 0 {
            return Err("port (integer 1-65535, got 0)".to_owned());
        }
        Ok(())
    }
}

struct UrlValidator;

impl Validator for UrlValidator {
    fn validate(&self, value: &str) -> Result<(), String> {
        if !value.contains("://") {
            return Err("URL (missing scheme, e.g. https://)".to_owned());
        }
        let after_scheme = value.split_once("://").map(|(_, rest)| rest).unwrap_or("");
        if after_scheme.is_empty() {
            return Err("URL (missing host after scheme)".to_owned());
        }
        Ok(())
    }
}

struct EnumValidator<'a> {
    variants: &'a [String],
}

impl Validator for EnumValidator<'_> {
    fn validate(&self, value: &str) -> Result<(), String> {
        if self.variants.iter().any(|v| v == value) {
            return Ok(());
        }
        Err(format!("one of [{}]", self.variants.join(", ")))
    }
}

struct StringValidator {
    min_len: Option<usize>,
    max_len: Option<usize>,
}

impl Validator for StringValidator {
    fn validate(&self, value: &str) -> Result<(), String> {
        let len = value.len();
        if let Some(min) = self.min_len {
            if len < min {
                return Err(format!("string (min length {min}, got {len})"));
            }
        }
        if let Some(max) = self.max_len {
            if len > max {
                return Err(format!("string (max length {max}, got {len})"));
            }
        }
        Ok(())
    }
}

struct PatternValidator<'a> {
    pattern: &'a str,
}

impl Validator for PatternValidator<'_> {
    fn validate(&self, value: &str) -> Result<(), String> {
        let regex = Regex::new(self.pattern)
            .map_err(|error| format!("pattern /{}/ (invalid regex: {error})", self.pattern))?;
        if regex.is_match(value) {
            return Ok(());
        }
        Err(format!("pattern /{}/", self.pattern))
    }
}

fn collect_validators<'a>(annotations: &'a EntryAnnotations) -> Vec<Box<dyn Validator + 'a>> {
    let mut validators: Vec<Box<dyn Validator + 'a>> = Vec::new();

    if let Some(env_type) = &annotations.env_type {
        match env_type {
            EnvType::Port => validators.push(Box::new(PortValidator)),
            EnvType::Url => validators.push(Box::new(UrlValidator)),
            EnvType::String { min_len, max_len } => validators.push(Box::new(StringValidator {
                min_len: *min_len,
                max_len: *max_len,
            })),
            EnvType::Enum(variants) => validators.push(Box::new(EnumValidator { variants })),
        }
    }

    if let Some(pattern) = annotations.pattern.as_deref() {
        validators.push(Box::new(PatternValidator { pattern }));
    }

    validators
}

#[cfg(test)]
#[path = "tests_src/validator/tests.rs"]
mod tests;
