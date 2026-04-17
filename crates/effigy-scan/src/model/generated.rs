use serde::Serialize;

use super::common::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GeneratedAssetSeverity {
    Warning,
    High,
    Critical,
}

impl GeneratedAssetSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedAssetFinding {
    pub path: String,
    pub bytes: usize,
    pub severity: GeneratedAssetSeverity,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedAssetThresholds {
    pub warn: usize,
    pub high: usize,
    pub critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedAssetScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub candidate_files: usize,
    pub findings: Vec<GeneratedAssetFinding>,
    pub thresholds: GeneratedAssetThresholds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeneratedInSrcCategory {
    ContentMarker,
    GeneratedFilename,
    GeneratedPath,
    BundledArtifact,
}

impl GeneratedInSrcCategory {
    pub fn as_str(self) -> &'static str {
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
pub enum GeneratedInSrcSeverity {
    Warning,
    High,
    Critical,
}

impl GeneratedInSrcSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedInSrcFinding {
    pub path: String,
    pub category: GeneratedInSrcCategory,
    pub severity: GeneratedInSrcSeverity,
    pub reason: String,
    pub size_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedInSrcThresholds {
    pub warn: usize,
    pub high: usize,
    pub critical: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GeneratedInSrcScanResult {
    pub root: String,
    pub scanned_files: usize,
    pub candidate_files: usize,
    pub findings: Vec<GeneratedInSrcFinding>,
    pub thresholds: GeneratedInSrcThresholds,
    pub source_roots: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GeneratedAssetScanOptions {
    pub thresholds: GeneratedAssetThresholds,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GeneratedInSrcScanOptions {
    pub thresholds: GeneratedInSrcThresholds,
    pub source_roots: Vec<String>,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}
