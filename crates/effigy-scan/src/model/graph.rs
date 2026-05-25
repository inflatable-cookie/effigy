use std::collections::BTreeMap;

use serde::Serialize;

use super::common::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryViolationSeverity {
    Warning,
    High,
    Critical,
}

impl BoundaryViolationSeverity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryViolationFinding {
    pub source_layer: String,
    pub target_layer: String,
    pub edge_kind: String,
    pub source_path: String,
    pub source_line: usize,
    pub source_symbol: String,
    pub target_path: String,
    pub target_line: usize,
    pub target_symbol: String,
    pub confidence: String,
    pub severity: BoundaryViolationSeverity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundaryViolationScanResult {
    pub root: String,
    pub configured_layers: usize,
    pub checked_edges: usize,
    pub findings: Vec<BoundaryViolationFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundaryLayerRule {
    pub paths: Vec<String>,
    pub may_depend_on: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BoundaryViolationScanOptions {
    pub include_heuristic: bool,
    pub layers: BTreeMap<String, BoundaryLayerRule>,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

impl Default for BoundaryViolationScanOptions {
    fn default() -> Self {
        Self {
            include_heuristic: false,
            layers: BTreeMap::new(),
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: false,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadCodeFindingKind {
    IsolatedFile,
    UnreferencedSymbol,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeadCodeSeverity {
    Warning,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeadCodeConfidence {
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadCodeFinding {
    pub kind: DeadCodeFindingKind,
    pub path: String,
    pub line: usize,
    pub symbol: Option<String>,
    pub symbol_kind: Option<String>,
    pub language_id: String,
    pub confidence: DeadCodeConfidence,
    pub severity: DeadCodeSeverity,
    pub reason: String,
    pub inbound_edges: usize,
    pub outbound_edges: usize,
    pub inbound_references: usize,
    pub outbound_references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeadCodeScanResult {
    pub root: String,
    pub checked_files: usize,
    pub checked_symbols: usize,
    pub skipped_allowlisted_paths: usize,
    pub skipped_allowlisted_symbols: usize,
    pub skipped_non_implementation_files: usize,
    pub skipped_unsupported_language_files: usize,
    pub findings: Vec<DeadCodeFinding>,
}

#[derive(Debug, Clone)]
pub struct DeadCodeScanOptions {
    pub include_heuristic: bool,
    pub allow_paths: Vec<String>,
    pub allow_symbols: Vec<String>,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

impl Default for DeadCodeScanOptions {
    fn default() -> Self {
        Self {
            include_heuristic: false,
            allow_paths: Vec::new(),
            allow_symbols: Vec::new(),
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: false,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationGapFindingKind {
    HotspotWithoutNearbyTests,
    ChangedOwnerWithoutTestTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ValidationGapSeverity {
    Warning,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ValidationGapConfidence {
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationGapTestTarget {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub confidence: String,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationGapFinding {
    pub kind: ValidationGapFindingKind,
    pub path: String,
    pub line: usize,
    pub language_id: String,
    pub confidence: ValidationGapConfidence,
    pub severity: ValidationGapSeverity,
    pub reason: String,
    pub connectivity: usize,
    pub inbound_edges: usize,
    pub outbound_edges: usize,
    pub inbound_references: usize,
    pub outbound_references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationGapScanResult {
    pub root: String,
    pub mode: String,
    pub hotspot_threshold: usize,
    pub affected_depth: usize,
    pub changed_paths: Vec<String>,
    pub checked_files: usize,
    pub skipped_allowlisted_paths: usize,
    pub skipped_non_implementation_files: usize,
    pub skipped_unsupported_language_files: usize,
    pub likely_test_files: Vec<ValidationGapTestTarget>,
    pub likely_test_tasks: Vec<ValidationGapTestTarget>,
    pub findings: Vec<ValidationGapFinding>,
}

#[derive(Debug, Clone)]
pub struct ValidationGapScanOptions {
    pub include_heuristic: bool,
    pub allow_paths: Vec<String>,
    pub hotspot_threshold: usize,
    pub affected_depth: usize,
    pub fail_on_findings: bool,
    pub respect_gitignore: bool,
    pub doctor_enabled: bool,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub format: ScanRenderFormat,
    pub out: Option<String>,
}

impl Default for ValidationGapScanOptions {
    fn default() -> Self {
        Self {
            include_heuristic: false,
            allow_paths: Vec::new(),
            hotspot_threshold: 4,
            affected_depth: 2,
            fail_on_findings: false,
            respect_gitignore: true,
            doctor_enabled: false,
            include: Vec::new(),
            exclude: Vec::new(),
            format: ScanRenderFormat::Text,
            out: None,
        }
    }
}
