use std::path::Path;

use super::super::request::ScanRequest;
use super::core::{
    apply_threshold_request_overrides, run_scan_mode, ScanExecution, ScanModeConfig,
};
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{
    catalog_scan_roots, load_root_duplicate_block_options, load_root_god_file_options,
    render_duplicate_block_markdown, render_duplicate_block_text, render_god_file_markdown,
    render_god_file_text, run_duplicate_block_scan_workspace, run_god_file_scan_workspace,
};

pub(super) fn run_god_files(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        ScanExecution {
            request,
            target_root,
            scan_roots: &scan_roots,
            mode: ScanModeConfig::new("god-files", "effigy.scan.god-files.v1"),
        },
        |root| load_root_god_file_options(root).map_err(Into::into),
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_god_file_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_god_file_text,
        render_god_file_markdown,
    )
}

pub(super) fn run_duplicate_blocks(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        ScanExecution {
            request,
            target_root,
            scan_roots: &scan_roots,
            mode: ScanModeConfig::new("duplicate-blocks", "effigy.scan.duplicate-blocks.v1"),
        },
        |root| load_root_duplicate_block_options(root).map_err(Into::into),
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_duplicate_block_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_duplicate_block_text,
        render_duplicate_block_markdown,
    )
}
