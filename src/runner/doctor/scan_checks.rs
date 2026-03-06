use std::path::{Path, PathBuf};

use super::super::model::catalog::LoadedCatalog;
use super::super::scan::model::{
    format_bytes, format_ratio, AttentionMarkerFinding, AttentionMarkerScanOptions,
    AttentionMarkerScanResult, AttentionMarkerSeverity, CommentRatioFinding,
    CommentRatioScanOptions, CommentRatioScanResult, CommentRatioSeverity, DuplicateBlockFinding,
    DuplicateBlockScanOptions, DuplicateBlockScanResult, DuplicateBlockSeverity,
    GeneratedAssetFinding, GeneratedAssetScanOptions, GeneratedAssetScanResult,
    GeneratedAssetSeverity, GeneratedInSrcFinding, GeneratedInSrcScanOptions,
    GeneratedInSrcScanResult, GeneratedInSrcSeverity, GodFileFinding, GodFileScanOptions,
    GodFileScanResult, GodFileSeverity, StaleSuppressionFinding, StaleSuppressionScanOptions,
    StaleSuppressionScanResult, StaleSuppressionSeverity,
};
use super::super::scan::options::catalog_scan_roots;
use super::report::{DoctorSeverity, DoctorState};
use crate::runner::error::RunnerError;

pub(super) struct ScanDoctorCheck {
    pub(super) check_id: &'static str,
    pub(super) label: &'static str,
    pub(super) remediation: &'static str,
}

pub(super) fn run_scan_check<TOptions, TResult, FLoad, FRun>(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
    check: ScanDoctorCheck,
    load_options: FLoad,
    run_scan: FRun,
) where
    TOptions: DoctorIntegratedScanOptions,
    TResult: DoctorIntegratedScanResult,
    FLoad: FnOnce(&Path, &[LoadedCatalog]) -> Result<TOptions, RunnerError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, RunnerError>,
{
    let options = match load_options(resolved_root, catalogs) {
        Ok(options) => options,
        Err(error) => {
            state.add_check_error(
                check.check_id,
                format!("{} configuration is invalid: {error}", check.label),
                "Fix manifest parse/schema errors first, then re-run `effigy doctor`.",
            );
            return;
        }
    };
    if !options.doctor_enabled() {
        return;
    }

    let scan_roots = catalog_scan_roots(resolved_root, catalogs);
    let result = match run_scan(resolved_root, &scan_roots, &options) {
        Ok(result) => result,
        Err(error) => {
            state.add_check_error(
                check.check_id,
                format!("{} scan failed: {error}", check.label),
                "No action required.",
            );
            return;
        }
    };

    for finding in result.into_findings() {
        state.add_check_finding(
            check.check_id,
            finding.doctor_severity(),
            finding.doctor_evidence(),
            check.remediation,
            false,
        );
    }
}

pub(super) trait DoctorIntegratedScanOptions {
    fn doctor_enabled(&self) -> bool;
}

pub(super) trait DoctorIntegratedScanResult {
    type Finding: DoctorIntegratedScanFinding;

    fn into_findings(self) -> Vec<Self::Finding>;
}

pub(super) trait DoctorIntegratedScanFinding {
    fn doctor_severity(&self) -> DoctorSeverity;
    fn doctor_evidence(&self) -> String;
}

impl DoctorIntegratedScanOptions for GodFileScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for GeneratedAssetScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for GeneratedInSrcScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for DuplicateBlockScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for CommentRatioScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for AttentionMarkerScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanOptions for StaleSuppressionScanOptions {
    fn doctor_enabled(&self) -> bool {
        self.doctor_enabled
    }
}

impl DoctorIntegratedScanResult for GodFileScanResult {
    type Finding = GodFileFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for GeneratedAssetScanResult {
    type Finding = GeneratedAssetFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for GeneratedInSrcScanResult {
    type Finding = GeneratedInSrcFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for DuplicateBlockScanResult {
    type Finding = DuplicateBlockFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for CommentRatioScanResult {
    type Finding = CommentRatioFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for AttentionMarkerScanResult {
    type Finding = AttentionMarkerFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanResult for StaleSuppressionScanResult {
    type Finding = StaleSuppressionFinding;

    fn into_findings(self) -> Vec<Self::Finding> {
        self.findings
    }
}

impl DoctorIntegratedScanFinding for GodFileFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GodFileSeverity::Warning => DoctorSeverity::Warning,
            GodFileSeverity::High | GodFileSeverity::Critical => DoctorSeverity::Error,
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} code lines ({} total) [{}] {}",
            self.code_lines,
            self.total_lines,
            self.severity.as_str(),
            self.path
        )
    }
}

impl DoctorIntegratedScanFinding for GeneratedAssetFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GeneratedAssetSeverity::Warning => DoctorSeverity::Warning,
            GeneratedAssetSeverity::High | GeneratedAssetSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} [{}] {} ({})",
            format_bytes(self.bytes),
            self.severity.as_str(),
            self.path,
            self.reason
        )
    }
}

impl DoctorIntegratedScanFinding for GeneratedInSrcFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            GeneratedInSrcSeverity::Warning => DoctorSeverity::Warning,
            GeneratedInSrcSeverity::High | GeneratedInSrcSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{} [{}] {} ({}/{})",
            format_bytes(self.size_bytes),
            self.severity.as_str(),
            self.path,
            self.category.as_str(),
            self.reason
        )
    }
}

impl DoctorIntegratedScanFinding for DuplicateBlockFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            DuplicateBlockSeverity::Warning => DoctorSeverity::Warning,
            DuplicateBlockSeverity::High | DuplicateBlockSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        let locations = self
            .locations
            .iter()
            .map(|location| {
                format!(
                    "{}:{}-{}",
                    location.path, location.start_line, location.end_line
                )
            })
            .collect::<Vec<String>>()
            .join(", ");
        format!(
            "{} lines [{}] {} occurrences ({})",
            self.block_lines,
            self.severity.as_str(),
            self.occurrences,
            locations
        )
    }
}

impl DoctorIntegratedScanFinding for CommentRatioFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            CommentRatioSeverity::Warning => DoctorSeverity::Warning,
            CommentRatioSeverity::High | CommentRatioSeverity::Critical => DoctorSeverity::Error,
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "ratio={} [{}] {} comment / {} code ({})",
            format_ratio(self.ratio),
            self.severity.as_str(),
            self.comment_lines,
            self.code_lines,
            self.path
        )
    }
}

impl DoctorIntegratedScanFinding for AttentionMarkerFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            AttentionMarkerSeverity::Warning => DoctorSeverity::Warning,
            AttentionMarkerSeverity::High | AttentionMarkerSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{}:{} [{}] {} [{}] {}",
            self.path,
            self.line,
            self.severity.as_str(),
            self.category.as_str(),
            self.marker,
            self.snippet
        )
    }
}

impl DoctorIntegratedScanFinding for StaleSuppressionFinding {
    fn doctor_severity(&self) -> DoctorSeverity {
        match self.severity {
            StaleSuppressionSeverity::Warning => DoctorSeverity::Warning,
            StaleSuppressionSeverity::High | StaleSuppressionSeverity::Critical => {
                DoctorSeverity::Error
            }
        }
    }

    fn doctor_evidence(&self) -> String {
        format!(
            "{}:{} [{}] {} [{}] {}",
            self.path,
            self.line,
            self.severity.as_str(),
            self.category.as_str(),
            self.marker,
            self.snippet
        )
    }
}
