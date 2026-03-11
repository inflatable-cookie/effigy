//! Changelog parsing, formatting, validation, analysis, and extraction.
//!
//! Implements the Northstar Changelog Profile, a strict subset of
//! [Keep a Changelog 1.0.0](https://keepachangelog.com/en/1.0.0/) designed for
//! machine-parseable changelogs with automated validation, formatting, release
//! version analysis, and release-note extraction.
//!
//! Typical flow:
//!
//! 1. [`load`] or [`parse`] changelog content
//! 2. [`validate`] it against the Northstar profile
//! 3. [`analyze`] the `Unreleased` section for the suggested bump
//! 4. [`extract_version`] when you need release-note source material for a
//!    specific version
//! 5. [`format()`] to normalize layout into canonical profile form
//!
//! CLI users should prefer:
//!
//! - `effigy changelog validate`
//! - `effigy changelog format`
//! - `effigy changelog analyze`
//! - `effigy changelog extract`
//!
//! For end-to-end operator guidance, see
//! `docs/guides/052-changelog-workflows-and-northstar-profile.md`.

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

/// Parse changelog content from a string into a [`Changelog`] AST.
///
/// # Examples
///
/// ```
/// use effigy::changelog;
///
/// let raw = "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Added
/// - New command
///
/// ### [0.2.5] - 2026-03-11
///
/// #### Fixed
/// - Prior bug fix
/// ";
///
/// let parsed = changelog::parse(raw).expect("valid changelog");
/// assert_eq!(parsed.title, "Changelog");
/// assert!(parsed.unreleased().is_some());
/// ```
pub fn parse(content: &str) -> Result<Changelog, ChangelogError> {
    parser::parse_changelog(content)
}

/// Format a parsed changelog into canonical Northstar Profile form.
///
/// Removes empty sections, normalizes spacing, and enforces category ordering.
///
/// # Examples
///
/// ```
/// use effigy::changelog;
///
/// let raw = "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Fixed
/// - Bug fix
///
/// #### Added
/// - Feature
/// ";
///
/// let parsed = changelog::parse(raw).expect("valid changelog");
/// let formatted = changelog::format(&parsed);
/// assert!(formatted.contains("### Added"));
/// assert!(formatted.contains("### Fixed"));
/// ```
pub fn format(changelog: &Changelog) -> String {
    formatter::format_changelog(changelog)
}

/// Validate a changelog against the Northstar Profile.
///
/// Returns a list of diagnostics. An empty list means the changelog is
/// fully compliant.
///
/// # Examples
///
/// ```
/// use effigy::changelog;
///
/// let raw = "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Added
/// - Feature
/// ";
///
/// let parsed = changelog::parse(raw).expect("valid changelog");
/// let diagnostics = changelog::validate(&parsed, raw);
/// assert!(diagnostics.is_empty());
/// ```
pub fn validate(changelog: &Changelog, raw_content: &str) -> Vec<ValidationDiagnostic> {
    validator::validate_changelog(changelog, raw_content)
}

/// Analyze a changelog and return version bump suggestions for `Unreleased`.
///
/// # Examples
///
/// ```
/// use effigy::changelog::{self, BumpKind};
///
/// let raw = "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Added
/// - Feature
///
/// ### [0.2.5] - 2026-03-11
///
/// #### Fixed
/// - Prior bug fix
/// ";
///
/// let parsed = changelog::parse(raw).expect("valid changelog");
/// let analysis = changelog::analyze(&parsed);
/// assert_eq!(analysis.suggested_bump, BumpKind::Patch);
/// assert_eq!(analysis.next_version.unwrap().to_string(), "0.2.6");
/// ```
pub fn analyze(changelog: &Changelog) -> Analysis {
    analyzer::analyze_changelog(changelog)
}

/// Extract release notes for a specific version as markdown.
///
/// Accepts `"Unreleased"` or a semver version string like `"0.2.0"`.
/// Returns `None` if the version is not found or has no entries.
///
/// # Examples
///
/// ```
/// use effigy::changelog;
///
/// let raw = "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Added
/// - New feature
///
/// ### [0.2.5] - 2026-03-11
///
/// #### Fixed
/// - Prior bug fix
/// ";
///
/// let parsed = changelog::parse(raw).expect("valid changelog");
/// let notes = changelog::extract_version(&parsed, "0.2.5").expect("release notes");
/// assert!(notes.contains("### Fixed"));
/// assert!(notes.contains("Prior bug fix"));
/// ```
pub fn extract_version(changelog: &Changelog, version: &str) -> Option<String> {
    extractor::extract_version(changelog, version)
}

/// Read and parse a changelog file from disk.
///
/// Use this when the changelog already lives on disk and you want the same AST
/// produced by [`parse`] without manually reading the file first.
///
/// # Examples
///
/// ```
/// use effigy::changelog;
/// use std::fs;
///
/// let path = std::env::temp_dir().join("effigy-rustdoc-load-example.md");
/// fs::write(
///     &path,
///     "\
/// ## Changelog
///
/// ### [Unreleased]
///
/// #### Fixed
/// - Example
/// ",
/// )
/// .expect("write example changelog");
///
/// let parsed = changelog::load(&path).expect("load changelog from disk");
/// assert_eq!(parsed.title, "Changelog");
///
/// let _ = fs::remove_file(&path);
/// ```
pub fn load(path: &Path) -> Result<Changelog, ChangelogError> {
    let content = std::fs::read_to_string(path).map_err(|error| ChangelogError::Io {
        path: path.to_owned(),
        error,
    })?;
    parser::parse_changelog(&content)
}
