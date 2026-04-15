//! Error types for env-schema parsing, resolution, and validation.

use std::path::PathBuf;
use std::time::Duration;

/// Errors that can occur during env-schema loading, resolution, or validation.
#[derive(Debug)]
pub enum EnvSchemaError {
    Io {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        line: usize,
        message: String,
    },
    ExecSpawn {
        command: String,
        error: std::io::Error,
    },
    ExecTimeout {
        command: String,
        timeout: Duration,
    },
    ExecFailed {
        command: String,
        code: Option<i32>,
        stderr: String,
    },
    CircularDependency {
        chain: Vec<String>,
    },
    Resolution {
        key: String,
        message: String,
    },
    MissingRequired {
        key: String,
        line: usize,
    },
    Validation {
        errors: Vec<ValidationError>,
    },
}

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub key: String,
    pub expected: String,
    pub actual: String,
    pub line: usize,
}

impl ValidationError {
    pub fn new(key: String, expected: String, actual: &str, line: usize, sensitive: bool) -> Self {
        Self {
            key,
            expected,
            actual: if sensitive {
                "[REDACTED]".to_owned()
            } else {
                actual.to_owned()
            },
            line,
        }
    }
}

impl std::fmt::Display for EnvSchemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EnvSchemaError::Io { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            EnvSchemaError::Parse {
                path,
                line,
                message,
            } => {
                write!(f, "{}:{line}: {message}", path.display())
            }
            EnvSchemaError::ExecSpawn { command, error } => {
                write!(f, "failed to spawn exec command `{command}`: {error}")
            }
            EnvSchemaError::ExecTimeout { command, timeout } => {
                write!(
                    f,
                    "exec command `{command}` timed out after {}s",
                    timeout.as_secs()
                )
            }
            EnvSchemaError::ExecFailed {
                command,
                code,
                stderr,
            } => {
                let code_text = code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_owned());
                if stderr.is_empty() {
                    write!(f, "exec command `{command}` failed (exit {code_text})")
                } else {
                    write!(
                        f,
                        "exec command `{command}` failed (exit {code_text}): {stderr}"
                    )
                }
            }
            EnvSchemaError::CircularDependency { chain } => {
                write!(
                    f,
                    "circular dependency in env schema: {}",
                    chain.join(" -> ")
                )
            }
            EnvSchemaError::Resolution { key, message } => {
                write!(f, "failed to resolve `{key}`: {message}")
            }
            EnvSchemaError::MissingRequired { key, line } => {
                write!(f, "line {line}: required variable `{key}` has no value")
            }
            EnvSchemaError::Validation { errors } => {
                let lines: Vec<String> = errors
                    .iter()
                    .map(|e| {
                        format!(
                            "line {}: `{}` expected {}, got `{}`",
                            e.line, e.key, e.expected, e.actual
                        )
                    })
                    .collect();
                write!(f, "env schema validation failed:\n  {}", lines.join("\n  "))
            }
        }
    }
}

impl std::error::Error for EnvSchemaError {}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}: `{}` expected {}, got `{}`",
            self.line, self.key, self.expected, self.actual
        )
    }
}
