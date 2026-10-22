use serde::Serialize;

use super::common::ScanRenderFormat;

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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GodFileThresholds {
    pub warn: usize,
    pub high: usize,
    pub critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GodFileScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub skipped_generated: usize,
    pub findings: Vec<GodFileFinding>,
    pub thresholds: GodFileThresholds,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentRatioSeverity {
    Warning,
    High,
    Critical,
}

impl CommentRatioSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommentRatioFinding {
    pub path: String,
    pub code_lines: usize,
    pub comment_lines: usize,
    pub ratio: f64,
    pub severity: CommentRatioSeverity,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommentRatioThresholds {
    pub warn: f64,
    pub high: f64,
    pub critical: f64,
    pub min_code_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CommentRatioScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub candidate_files: usize,
    pub findings: Vec<CommentRatioFinding>,
    pub thresholds: CommentRatioThresholds,
}

#[derive(Debug, Clone)]
pub struct GodFileScanOptions {
    pub thresholds: GodFileThresholds,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
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

#[derive(Debug, Clone)]
pub struct CommentRatioScanOptions {
    pub thresholds: CommentRatioThresholds,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}
