use serde::Serialize;

use super::common::ScanRenderFormat;

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
pub(in crate::runner) enum GeneratedInSrcCategory {
    ContentMarker,
    GeneratedFilename,
    GeneratedPath,
    BundledArtifact,
}

impl GeneratedInSrcCategory {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::ContentMarker => "content-marker",
            Self::GeneratedFilename => "generated-filename",
            Self::GeneratedPath => "generated-path",
            Self::BundledArtifact => "bundled-artifact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::runner) enum GeneratedInSrcSeverity {
    Warning,
    High,
    Critical,
}

impl GeneratedInSrcSeverity {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedInSrcFinding {
    pub(in crate::runner) path: String,
    pub(in crate::runner) category: GeneratedInSrcCategory,
    pub(in crate::runner) severity: GeneratedInSrcSeverity,
    pub(in crate::runner) reason: String,
    pub(in crate::runner) size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedInSrcThresholds {
    pub(in crate::runner) warn: usize,
    pub(in crate::runner) high: usize,
    pub(in crate::runner) critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::runner) struct GeneratedInSrcScanResult {
    pub(in crate::runner) root: String,
    pub(in crate::runner) scanned_files: usize,
    pub(in crate::runner) candidate_files: usize,
    pub(in crate::runner) findings: Vec<GeneratedInSrcFinding>,
    pub(in crate::runner) thresholds: GeneratedInSrcThresholds,
    pub(in crate::runner) source_roots: Vec<String>,
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
pub(in crate::runner) struct GeneratedInSrcScanOptions {
    pub(in crate::runner) thresholds: GeneratedInSrcThresholds,
    pub(in crate::runner) source_roots: Vec<String>,
    pub(in crate::runner) fail_on_findings: bool,
    pub(in crate::runner) respect_gitignore: bool,
    pub(in crate::runner) doctor_enabled: bool,
    pub(in crate::runner) include: Vec<String>,
    pub(in crate::runner) exclude: Vec<String>,
    pub(in crate::runner) format: ScanRenderFormat,
    pub(in crate::runner) out: Option<String>,
}
