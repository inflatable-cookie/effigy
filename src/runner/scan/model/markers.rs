use serde::Serialize;

use super::common::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum AttentionMarkerCategory {
    DeferredWork,
    Deprecation,
    TemporaryArtifact,
}

impl AttentionMarkerCategory {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::DeferredWork => "deferred-work",
            Self::Deprecation => "deprecation",
            Self::TemporaryArtifact => "temporary-artifact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum AttentionMarkerSeverity {
    Warning,
    High,
    Critical,
}

impl AttentionMarkerSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct AttentionMarkerFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) line: usize,
    pub(in crate::runner) category: AttentionMarkerCategory,
    pub(in crate::runner) severity: AttentionMarkerSeverity,
    pub(in crate::runner) marker: String,
    pub(in crate::runner) snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct AttentionMarkerPatterns {
    pub(in crate::runner) warning: Vec<String>,
    pub(in crate::runner) high: Vec<String>,
    pub(in crate::runner) critical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct AttentionMarkerScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) matched_lines: usize,
    pub(in crate::runner) findings: Vec<AttentionMarkerFinding>,
    pub(in crate::runner) patterns: AttentionMarkerPatterns,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum StaleSuppressionCategory {
    TypeIgnore,
    LintDisable,
    ToolBypass,
}

impl StaleSuppressionCategory {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::TypeIgnore => "type-ignore",
            Self::LintDisable => "lint-disable",
            Self::ToolBypass => "tool-bypass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum StaleSuppressionSeverity {
    Warning,
    High,
    Critical,
}

impl StaleSuppressionSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct StaleSuppressionFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) line: usize,
    pub(in crate::runner) category: StaleSuppressionCategory,
    pub(in crate::runner) severity: StaleSuppressionSeverity,
    pub(in crate::runner) marker: String,
    pub(in crate::runner) snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct StaleSuppressionPatterns {
    pub(in crate::runner) warning: Vec<String>,
    pub(in crate::runner) high: Vec<String>,
    pub(in crate::runner) critical: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct StaleSuppressionScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) matched_lines: usize,
    pub(in crate::runner) findings: Vec<StaleSuppressionFinding>,
    pub(in crate::runner) patterns: StaleSuppressionPatterns,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct AttentionMarkerScanOptions {
    pub(in crate::runner) patterns: AttentionMarkerPatterns,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct StaleSuppressionScanOptions {
    pub(in crate::runner) patterns: StaleSuppressionPatterns,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}
