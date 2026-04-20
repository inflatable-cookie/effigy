//! Changelog parsing, formatting, validation, analysis, and extraction for the
//! Northstar changelog profile.

pub mod error;
pub mod types;

mod analyzer;
mod extractor;
mod formatter;
mod parser;
mod validator;

use std::path::Path;

pub use analyzer::{Analysis, BumpKind};
pub use error::{ChangelogError, ParseDiagnostic, ValidationDiagnostic};
pub use types::*;

/// Parse changelog content into a [`Changelog`] AST.
pub fn parse(content: &str) -> Result<Changelog, ChangelogError> {
    parser::parse_changelog(content)
}

/// Format a changelog into canonical Northstar profile layout.
pub fn format(changelog: &Changelog) -> String {
    formatter::format_changelog(changelog)
}

/// Validate changelog content and return any profile diagnostics.
pub fn validate(changelog: &Changelog, raw_content: &str) -> Vec<ValidationDiagnostic> {
    validator::validate_changelog(changelog, raw_content)
}

/// Analyze `Unreleased` and return the suggested semantic version bump.
pub fn analyze(changelog: &Changelog) -> Analysis {
    analyzer::analyze_changelog(changelog)
}

/// Extract markdown release notes for `Unreleased` or a concrete version.
pub fn extract_version(changelog: &Changelog, version: &str) -> Option<String> {
    extractor::extract_version(changelog, version)
}

/// Read and parse a changelog file from disk.
pub fn load(path: &Path) -> Result<Changelog, ChangelogError> {
    let content = std::fs::read_to_string(path).map_err(|error| ChangelogError::Io {
        path: path.to_owned(),
        error,
    })?;
    parser::parse_changelog(&content)
}
