use serde::Serialize;

use super::common::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum GodFileSeverity {
    Warning,
    High,
    Critical,
}

impl GodFileSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GodFileFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) code_lines: usize,
    pub(in crate::runner) total_lines: usize,
    pub(in crate::runner) severity: GodFileSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GodFileThresholds {
    pub(in crate::runner) warn: usize,
    pub(in crate::runner) high: usize,
    pub(in crate::runner) critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GodFileScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) skipped_generated: usize,
    pub(in crate::runner) findings: Vec<GodFileFinding>,
    pub(in crate::runner) thresholds: GodFileThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum DuplicateBlockSeverity {
    Warning,
    High,
    Critical,
}

impl DuplicateBlockSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct DuplicateBlockLocation {
    pub(in crate::runner) path: String,
    pub(in crate::runner) start_line: usize,
    pub(in crate::runner) end_line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct DuplicateBlockFinding {
    pub(in crate::runner) severity: DuplicateBlockSeverity,
    pub(in crate::runner) block_lines: usize,
    pub(in crate::runner) occurrences: usize,
    pub(in crate::runner) fingerprint: String,
    pub(in crate::runner) snippet: String,
    pub(in crate::runner) locations: Vec<DuplicateBlockLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct DuplicateBlockThresholds {
    pub(in crate::runner) warn: usize,
    pub(in crate::runner) high: usize,
    pub(in crate::runner) critical: usize,
    pub(in crate::runner) min_occurrences: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct DuplicateBlockScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) candidate_blocks: usize,
    pub(in crate::runner) findings: Vec<DuplicateBlockFinding>,
    pub(in crate::runner) thresholds: DuplicateBlockThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum CommentRatioSeverity {
    Warning,
    High,
    Critical,
}

impl CommentRatioSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::runner) struct CommentRatioFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) code_lines: usize,
    pub(in crate::runner) comment_lines: usize,
    pub(in crate::runner) ratio: f64,
    pub(in crate::runner) severity: CommentRatioSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::runner) struct CommentRatioThresholds {
    pub(in crate::runner) warn: f64,
    pub(in crate::runner) high: f64,
    pub(in crate::runner) critical: f64,
    pub(in crate::runner) min_code_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(in crate::runner) struct CommentRatioScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) candidate_files: usize,
    pub(in crate::runner) findings: Vec<CommentRatioFinding>,
    pub(in crate::runner) thresholds: CommentRatioThresholds,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct GodFileScanOptions {
    pub(in crate::runner) thresholds: GodFileThresholds,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct DuplicateBlockScanOptions {
    pub(in crate::runner) thresholds: DuplicateBlockThresholds,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}

#[derive(Debug, Clone)]
pub(in crate::runner) struct CommentRatioScanOptions {
    pub(in crate::runner) thresholds: CommentRatioThresholds,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}
