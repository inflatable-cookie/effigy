use serde::Serialize;

use super::common::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AttentionMarkerCategory {
    DeferredWork,
    Deprecation,
    TemporaryArtifact,
}

impl AttentionMarkerCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeferredWork => "deferred-work",
            Self::Deprecation => "deprecation",
            Self::TemporaryArtifact => "temporary-artifact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AttentionMarkerSeverity {
    Warning,
    High,
    Critical,
}

impl AttentionMarkerSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttentionMarkerFinding {
    pub path: String,
    pub line: usize,
    pub category: AttentionMarkerCategory,
    pub severity: AttentionMarkerSeverity,
    pub marker: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttentionMarkerPatterns {
    pub warning: Vec<String>,
    pub high: Vec<String>,
    pub critical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttentionMarkerScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub matched_lines: usize,
    pub findings: Vec<AttentionMarkerFinding>,
    pub patterns: AttentionMarkerPatterns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StaleSuppressionCategory {
    TypeIgnore,
    LintDisable,
    ToolBypass,
}

impl StaleSuppressionCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TypeIgnore => "type-ignore",
            Self::LintDisable => "lint-disable",
            Self::ToolBypass => "tool-bypass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StaleSuppressionSeverity {
    Warning,
    High,
    Critical,
}

impl StaleSuppressionSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleSuppressionFinding {
    pub path: String,
    pub line: usize,
    pub category: StaleSuppressionCategory,
    pub severity: StaleSuppressionSeverity,
    pub marker: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleSuppressionPatterns {
    pub warning: Vec<String>,
    pub high: Vec<String>,
    pub critical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StaleSuppressionScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub matched_lines: usize,
    pub findings: Vec<StaleSuppressionFinding>,
    pub patterns: StaleSuppressionPatterns,
}

#[derive(Debug, Clone)]
pub struct AttentionMarkerScanOptions {
    pub patterns: AttentionMarkerPatterns,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StaleSuppressionScanOptions {
    pub patterns: StaleSuppressionPatterns,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}
