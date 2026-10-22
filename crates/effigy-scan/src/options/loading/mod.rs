use std::path::{Path, PathBuf};

use crate::error::ScanError;
use effigy_manifest::LoadedCatalog;

use self::common::{doctor_manifest_options, load_root_manifest_options};
use super::super::model::{DuplicateBlockScanOptions, GodFileScanOptions};

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
