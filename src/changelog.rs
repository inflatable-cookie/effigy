//! Changelog parsing, formatting, validation, and analysis.
//!
//! Implements the Northstar Changelog Profile — a strict subset of
//! [Keep a Changelog 1.0.0](https://keepachangelog.com/en/1.0.0/) designed
//! for machine-parseable changelogs with automated validation, formatting,
//! and version analysis.

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

/// Parse changelog content from a string.
pub fn parse(content: &str) -> Result<Changelog, ChangelogError> {
    parser::parse_changelog(content)
}

/// Format a parsed changelog into canonical Northstar Profile form.
///
/// Removes empty sections, normalizes spacing, and enforces category ordering.
pub fn format(changelog: &Changelog) -> String {
    formatter::format_changelog(changelog)
}

/// Validate a changelog against the Northstar Profile.
///
/// Returns a list of diagnostics. An empty list means the changelog is
/// fully compliant.
pub fn validate(changelog: &Changelog, raw_content: &str) -> Vec<ValidationDiagnostic> {
    validator::validate_changelog(changelog, raw_content)
}

/// Analyze a changelog and return version bump suggestions.
pub fn analyze(changelog: &Changelog) -> Analysis {
    analyzer::analyze_changelog(changelog)
}

/// Extract release notes for a specific version as markdown.
///
/// Accepts `"Unreleased"` or a semver version string like `"0.2.0"`.
/// Returns `None` if the version is not found or has no entries.
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
