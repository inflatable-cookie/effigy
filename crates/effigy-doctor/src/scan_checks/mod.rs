use std::path::{Path, PathBuf};

use crate::DoctorError;
use crate::{DoctorSeverity, DoctorState};
use effigy_manifest::LoadedCatalog;
use effigy_scan::{
    catalog_scan_roots, format_bytes, format_ratio, AttentionMarkerFinding,
    AttentionMarkerScanOptions, AttentionMarkerScanResult, AttentionMarkerSeverity,
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, CommentRatioSeverity,
    DuplicateBlockFinding, DuplicateBlockScanOptions, DuplicateBlockScanResult,
    DuplicateBlockSeverity, GeneratedAssetFinding, GeneratedAssetScanOptions,
    GeneratedAssetScanResult, GeneratedAssetSeverity, GeneratedInSrcFinding,
    GeneratedInSrcScanOptions, GeneratedInSrcScanResult, GeneratedInSrcSeverity, GodFileFinding,
    GodFileScanOptions, GodFileScanResult, GodFileSeverity, StaleSuppressionFinding,
    StaleSuppressionScanOptions, StaleSuppressionScanResult, StaleSuppressionSeverity,
};

mod core;
mod findings;
mod integration;

pub(super) use core::ScanDoctorCheck;

pub(super) fn run_scan_check<TOptions, TResult, FLoad, FRun>(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
    check: ScanDoctorCheck,
    load_options: FLoad,
    run_scan: FRun,
) where
    TOptions: core::DoctorIntegratedScanOptions,
    TResult: core::DoctorIntegratedScanResult,
    FLoad: FnOnce(&Path, &[LoadedCatalog]) -> Result<TOptions, DoctorError>,
    FRun: FnOnce(&Path, &[PathBuf], &TOptions) -> Result<TResult, DoctorError>,
{
    core::run_scan_check(
        resolved_root,
        catalogs,
        state,
        check,
        load_options,
        run_scan,
    )
}
