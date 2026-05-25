use std::path::{Path, PathBuf};

use crate::error::ScanError;
use effigy_manifest::LoadedCatalog;

use self::common::{doctor_manifest_options, load_root_manifest_options};
use super::super::model::{
    AttentionMarkerScanOptions, BoundaryViolationScanOptions, CommentRatioScanOptions,
    DeadCodeScanOptions, DuplicateBlockScanOptions, GeneratedAssetScanOptions,
    GeneratedInSrcScanOptions, GodFileScanOptions, StaleSuppressionScanOptions,
    ValidationGapScanOptions,
};

mod common;
mod impls;
mod traits;

pub fn load_root_god_file_options(target_root: &Path) -> Result<GodFileScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub fn load_root_boundary_violation_options(
    target_root: &Path,
) -> Result<BoundaryViolationScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.boundary_violations.as_ref())
    })
}

pub fn load_root_dead_code_options(target_root: &Path) -> Result<DeadCodeScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.dead_code.as_ref())
    })
}

pub fn load_root_validation_gap_options(
    target_root: &Path,
) -> Result<ValidationGapScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.validation_gaps.as_ref())
    })
}

pub fn load_root_generated_asset_options(
    target_root: &Path,
) -> Result<GeneratedAssetScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub fn load_root_generated_in_src_options(
    target_root: &Path,
) -> Result<GeneratedInSrcScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub fn load_root_duplicate_block_options(
    target_root: &Path,
) -> Result<DuplicateBlockScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub fn load_root_comment_ratio_options(
    target_root: &Path,
) -> Result<CommentRatioScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub fn load_root_attention_marker_options(
    target_root: &Path,
) -> Result<AttentionMarkerScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub fn load_root_stale_suppression_options(
    target_root: &Path,
) -> Result<StaleSuppressionScanOptions, ScanError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub fn doctor_attention_marker_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<AttentionMarkerScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub fn doctor_stale_suppression_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<StaleSuppressionScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub fn doctor_generated_asset_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedAssetScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub fn doctor_generated_in_src_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedInSrcScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub fn doctor_duplicate_block_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<DuplicateBlockScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub fn doctor_comment_ratio_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<CommentRatioScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub fn doctor_dead_code_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<DeadCodeScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.dead_code.as_ref())
    })
}

pub fn doctor_validation_gap_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<ValidationGapScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.validation_gaps.as_ref())
    })
}

pub fn doctor_god_file_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GodFileScanOptions, ScanError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub fn catalog_scan_roots(target_root: &Path, catalogs: &[LoadedCatalog]) -> Vec<PathBuf> {
    let mut roots = catalogs
        .iter()
        .filter(|catalog| {
            catalog.catalog_root == target_root || catalog.catalog_root.starts_with(target_root)
        })
        .map(|catalog| catalog.catalog_root.clone())
        .collect::<Vec<PathBuf>>();
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        roots.push(target_root.to_path_buf());
    }
    roots
}
