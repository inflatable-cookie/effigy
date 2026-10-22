use std::ffi::OsStr;
use std::path::Path;

use serde::Serialize;

use super::common::ScanRenderFormat;
use crate::constants::{
    DEFAULT_GOD_FILES_CRITICAL, DEFAULT_GOD_FILES_HIGH, DEFAULT_GOD_FILES_WARN,
    GOD_FILE_LANGUAGE_PROFILES,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GodFileSeverity {
    Warning,
    High,
    Critical,
}

impl GodFileSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GodFileFinding {
    pub path: String,
    pub code_lines: usize,
    pub total_lines: usize,
    pub severity: GodFileSeverity,
    /// File extension the size thresholds were chosen for (`other` when the
    /// extension is not in a known language group).
    pub language: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct GodFileThresholds {
    pub warn: usize,
    pub high: usize,
    pub critical: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GodFileThresholdsSource {
    /// `warn`/`high`/`critical` came from the manifest or the command line.
    Configured,
    /// No explicit threshold was configured, so the file's language group
    /// supplied the defaults.
    LanguageDefaults,
}

impl GodFileThresholdsSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::LanguageDefaults => "language-defaults",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GodFileScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub skipped_generated: usize,
    pub findings: Vec<GodFileFinding>,
    /// The configured thresholds, or the `other`-language defaults when the
    /// per-language defaults are in effect.
    pub thresholds: GodFileThresholds,
    pub thresholds_source: GodFileThresholdsSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DuplicateBlockSeverity {
    Warning,
    High,
    Critical,
}

impl DuplicateBlockSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateBlockLocation {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateBlockFinding {
    pub severity: DuplicateBlockSeverity,
    pub block_lines: usize,
    pub occurrences: usize,
    pub fingerprint: String,
    pub snippet: String,
    pub locations: Vec<DuplicateBlockLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateBlockThresholds {
    pub warn: usize,
    pub high: usize,
    pub critical: usize,
    pub min_occurrences: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DuplicateBlockScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub candidate_blocks: usize,
    pub findings: Vec<DuplicateBlockFinding>,
    pub thresholds: DuplicateBlockThresholds,
}

#[derive(Debug, Clone)]
pub struct GodFileScanOptions {
    pub thresholds: GodFileThresholds,
    /// Set when the manifest or command line supplied any threshold, which
    /// overrides the per-language defaults.
    pub explicit_thresholds: bool,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

impl Default for GodFileThresholds {
    fn default() -> Self {
        Self {
            warn: DEFAULT_GOD_FILES_WARN,
            high: DEFAULT_GOD_FILES_HIGH,
            critical: DEFAULT_GOD_FILES_CRITICAL,
        }
    }
}

impl GodFileScanOptions {
    /// Effective thresholds for one file: explicit configuration wins, else the
    /// file's language group supplies its own defaults.
    pub fn thresholds_for(&self, path: &Path) -> GodFileThresholds {
        if self.explicit_thresholds {
            return self.thresholds;
        }
        let language = god_file_language(path);
        GOD_FILE_LANGUAGE_PROFILES
            .iter()
            .find(|profile| profile.extensions.contains(&language.as_str()))
            .map(|profile| GodFileThresholds {
                warn: profile.warn,
                high: profile.high,
                critical: profile.critical,
            })
            .unwrap_or(self.thresholds)
    }

    pub fn thresholds_source(&self) -> GodFileThresholdsSource {
        if self.explicit_thresholds {
            GodFileThresholdsSource::Configured
        } else {
            GodFileThresholdsSource::LanguageDefaults
        }
    }
}

/// Language key for a file, derived from its extension (`other` for unknown or
/// extensionless files).
pub fn god_file_language(path: &Path) -> String {
    path.extension()
        .and_then(OsStr::to_str)
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| !extension.is_empty())
        .unwrap_or_else(|| "other".to_owned())
}

#[derive(Debug, Clone)]
pub struct DuplicateBlockScanOptions {
    pub thresholds: DuplicateBlockThresholds,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}
