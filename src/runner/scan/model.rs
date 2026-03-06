use serde::Serialize;

use super::super::manifest::config_sections::ManifestScanOutputFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum ScanRenderFormat {
    Text,
    Markdown,
}

impl ScanRenderFormat {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Markdown => "markdown",
        }
    }
}

impl From<ManifestScanOutputFormat> for ScanRenderFormat {
    fn from(value: ManifestScanOutputFormat) -> Self {
        match value {
            ManifestScanOutputFormat::Text => Self::Text,
            ManifestScanOutputFormat::Markdown => Self::Markdown,
        }
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum GeneratedAssetSeverity {
    Warning,
    High,
    Critical,
}

impl GeneratedAssetSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedAssetFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) bytes: usize,
    pub(in crate::runner) severity: GeneratedAssetSeverity,
    pub(in crate::runner) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedAssetThresholds {
    pub(in crate::runner) warn: usize,
    pub(in crate::runner) high: usize,
    pub(in crate::runner) critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedAssetScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) candidate_files: usize,
    pub(in crate::runner) findings: Vec<GeneratedAssetFinding>,
    pub(in crate::runner) thresholds: GeneratedAssetThresholds,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct TextRenderOptions {
    pub(in crate::runner) show_warnings: bool,
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
pub(in crate::runner) struct GeneratedAssetScanOptions {
    pub(in crate::runner) thresholds: GeneratedAssetThresholds,
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

pub(in crate::runner) fn format_bytes(bytes: usize) -> String {
    if bytes >= 1_000_000_000 {
        return format!("{:.1} GB", bytes as f64 / 1_000_000_000f64);
    }
    if bytes >= 1_000_000 {
        return format!("{:.1} MB", bytes as f64 / 1_000_000f64);
    }
    if bytes >= 1_000 {
        return format!("{:.1} KB", bytes as f64 / 1_000f64);
    }
    format!("{bytes} B")
}

pub(in crate::runner) fn format_ratio(ratio: f64) -> String {
    format!("{ratio:.2}")
}
