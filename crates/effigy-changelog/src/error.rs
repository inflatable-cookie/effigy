//! Error types for changelog parsing, formatting, and validation.

use std::path::PathBuf;

/// Errors that can occur during changelog operations.
#[derive(Debug)]
pub enum ChangelogError {
    /// I/O error reading a changelog file.
    Io {
        /// Path that failed to load.
        path: PathBuf,
        /// Underlying filesystem error.
        error: std::io::Error,
    },
    /// One or more parse errors in the changelog content.
    Parse {
        /// Collected parse diagnostics.
        errors: Vec<ParseDiagnostic>,
    },
}

/// A single parse error with location information.
#[derive(Debug, Clone)]
pub struct ParseDiagnostic {
    /// Source line number (1-based).
    pub line: usize,
    /// Human-readable error message.
    pub message: String,
}

/// A single validation diagnostic with location and fix suggestion.
#[derive(Debug, Clone)]
pub struct ValidationDiagnostic {
    /// Source line number (1-based). May be 0 for file-level issues.
    pub line: usize,
    /// Short rule identifier (e.g., `"empty-section"`, `"duplicate-version"`).
    pub rule: &'static str,
    /// Human-readable description of the issue.
    pub message: String,
    /// Optional suggested fix.
    pub suggestion: Option<String>,
}

impl std::fmt::Display for ChangelogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangelogError::Io { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            ChangelogError::Parse { errors } => {
                let lines: Vec<String> = errors
                    .iter()
                    .map(|e| format!("  line {}: {}", e.line, e.message))
                    .collect();
                write!(f, "changelog parse errors:\n{}", lines.join("\n"))
            }
        }
    }
}

impl std::error::Error for ChangelogError {}

impl std::fmt::Display for ParseDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::fmt::Display for ValidationDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: [{}] {}", self.line, self.rule, self.message)?;
        if let Some(ref suggestion) = self.suggestion {
            write!(f, " (fix: {suggestion})")?;
        }
        Ok(())
    }
}
