use super::ast::{EnvSchema, EnvType};
use super::error::ValidationError;
use super::resolver::ResolvedEnv;

/// Validate resolved values against their declared types.
///
/// Returns a list of validation errors (empty if all values are valid).
pub fn validate_resolved_env(schema: &EnvSchema, resolved: &ResolvedEnv) -> Vec<ValidationError> {
    let mut errors = Vec::new();

    for entry in &schema.entries {
        let Some(env_type) = &entry.annotations.env_type else {
            continue;
        };
        let Some(value) = resolved.get_value(&entry.key) else {
            continue;
        };

        if let Err(expected) = validate_value(value, env_type) {
            errors.push(ValidationError {
                key: entry.key.clone(),
                expected,
                actual: value.to_owned(),
                line: entry.line,
            });
        }
    }

    errors
}

fn validate_value(value: &str, env_type: &EnvType) -> Result<(), String> {
    match env_type {
        EnvType::Port => validate_port(value),
        EnvType::Url => validate_url(value),
        EnvType::String { min_len, max_len } => validate_string(value, *min_len, *max_len),
        EnvType::Enum(variants) => validate_enum(value, variants),
    }
}

fn validate_port(value: &str) -> Result<(), String> {
    let n: u16 = value
        .parse()
        .map_err(|_| "port (integer 1-65535)".to_owned())?;
    if n == 0 {
        return Err("port (integer 1-65535, got 0)".to_owned());
    }
    Ok(())
}

fn validate_url(value: &str) -> Result<(), String> {
    if !value.contains("://") {
        return Err("URL (missing scheme, e.g. https://)".to_owned());
    }
    let after_scheme = value.split_once("://").map(|(_, rest)| rest).unwrap_or("");
    if after_scheme.is_empty() {
        return Err("URL (missing host after scheme)".to_owned());
    }
    Ok(())
}

fn validate_string(
    value: &str,
    min_len: Option<usize>,
    max_len: Option<usize>,
) -> Result<(), String> {
    let len = value.len();
    if let Some(min) = min_len {
        if len < min {
            return Err(format!("string (min length {min}, got {len})"));
        }
    }
    if let Some(max) = max_len {
        if len > max {
            return Err(format!("string (max length {max}, got {len})"));
        }
    }
    Ok(())
}

fn validate_enum(value: &str, variants: &[String]) -> Result<(), String> {
    if variants.iter().any(|v| v == value) {
        return Ok(());
    }
    Err(format!("one of [{}]", variants.join(", ")))
}

#[cfg(test)]
#[path = "validator/tests.rs"]
mod tests;
