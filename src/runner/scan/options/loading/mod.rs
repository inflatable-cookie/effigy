use std::path::{Path, PathBuf};

use crate::runner::error::RunnerError;
use crate::runner::model::catalog::LoadedCatalog;

use self::common::{doctor_manifest_options, load_root_manifest_options};
use super::super::model::{
    AttentionMarkerScanOptions, CommentRatioScanOptions, DuplicateBlockScanOptions,
    GeneratedAssetScanOptions, GeneratedInSrcScanOptions, GodFileScanOptions,
    StaleSuppressionScanOptions,
};

mod common;
mod impls;
mod traits;

pub(in crate::runner) fn load_root_god_file_options(
    target_root: &Path,
) -> Result<GodFileScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub(in crate::runner) fn load_root_generated_asset_options(
    target_root: &Path,
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub(in crate::runner) fn load_root_generated_in_src_options(
    target_root: &Path,
) -> Result<GeneratedInSrcScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub(in crate::runner) fn load_root_duplicate_block_options(
    target_root: &Path,
) -> Result<DuplicateBlockScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub(in crate::runner) fn load_root_comment_ratio_options(
    target_root: &Path,
) -> Result<CommentRatioScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub(in crate::runner) fn load_root_attention_marker_options(
    target_root: &Path,
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub(in crate::runner) fn load_root_stale_suppression_options(
    target_root: &Path,
) -> Result<StaleSuppressionScanOptions, RunnerError> {
    load_root_manifest_options(target_root, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub(in crate::runner) fn doctor_attention_marker_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<AttentionMarkerScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.attention_markers.as_ref())
    })
}

pub(in crate::runner) fn doctor_stale_suppression_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<StaleSuppressionScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.stale_suppressions.as_ref())
    })
}

pub(in crate::runner) fn doctor_generated_asset_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedAssetScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_assets.as_ref())
    })
}

pub(in crate::runner) fn doctor_generated_in_src_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GeneratedInSrcScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.generated_in_src.as_ref())
    })
}

pub(in crate::runner) fn doctor_duplicate_block_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<DuplicateBlockScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.duplicate_blocks.as_ref())
    })
}

pub(in crate::runner) fn doctor_comment_ratio_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<CommentRatioScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.comment_ratio.as_ref())
    })
}

pub(in crate::runner) fn doctor_god_file_options(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<GodFileScanOptions, RunnerError> {
    doctor_manifest_options(resolved_root, catalogs, |manifest| {
        manifest
            .scan
            .as_ref()
            .and_then(|scan| scan.god_files.as_ref())
    })
}

pub(in crate::runner) fn catalog_scan_roots(
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<PathBuf> {
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
