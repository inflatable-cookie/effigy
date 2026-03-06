use std::path::Path;

use super::super::super::super::model::catalog::LoadedCatalog;
use super::super::super::super::scan::execution::{
    run_attention_marker_scan_workspace, run_comment_ratio_scan_workspace,
    run_duplicate_block_scan_workspace, run_generated_asset_scan_workspace,
    run_god_file_scan_workspace,
};
use super::super::super::super::scan::options::{
    catalog_scan_roots, load_root_attention_marker_options, load_root_comment_ratio_options,
    load_root_duplicate_block_options, load_root_generated_asset_options,
    load_root_god_file_options,
};
use super::super::super::super::scan::render::{
    render_attention_marker_markdown, render_attention_marker_text, render_comment_ratio_markdown,
    render_comment_ratio_text, render_duplicate_block_markdown, render_duplicate_block_text,
    render_generated_asset_markdown, render_generated_asset_text, render_god_file_markdown,
    render_god_file_text,
};
use super::super::request::ScanRequest;
use super::core::{
    apply_comment_ratio_request_overrides, apply_common_request_overrides,
    apply_threshold_request_overrides, reject_threshold_overrides, run_scan_mode, ScanModeConfig,
};
use crate::runner::error::RunnerError;

pub(super) fn run_god_files(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        request,
        target_root,
        &scan_roots,
        ScanModeConfig::new("god-files", "effigy.scan.god-files.v1"),
        load_root_god_file_options,
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        run_god_file_scan_workspace,
        render_god_file_text,
        render_god_file_markdown,
    )
}

pub(super) fn run_duplicate_blocks(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        request,
        target_root,
        &scan_roots,
        ScanModeConfig::new("duplicate-blocks", "effigy.scan.duplicate-blocks.v1"),
        load_root_duplicate_block_options,
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        run_duplicate_block_scan_workspace,
        render_duplicate_block_text,
        render_duplicate_block_markdown,
    )
}

pub(super) fn run_comment_ratio(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        request,
        target_root,
        &scan_roots,
        ScanModeConfig::new("comment-ratio", "effigy.scan.comment-ratio.v1"),
        load_root_comment_ratio_options,
        |options, request| {
            apply_comment_ratio_request_overrides(options, request);
            Ok(())
        },
        run_comment_ratio_scan_workspace,
        render_comment_ratio_text,
        render_comment_ratio_markdown,
    )
}

pub(super) fn run_generated_assets(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        request,
        target_root,
        &scan_roots,
        ScanModeConfig::new("generated-assets", "effigy.scan.generated-assets.v1"),
        load_root_generated_asset_options,
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        run_generated_asset_scan_workspace,
        render_generated_asset_text,
        render_generated_asset_markdown,
    )
}

pub(super) fn run_attention_markers(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    run_scan_mode(
        request,
        target_root,
        &scan_roots,
        ScanModeConfig::new("attention-markers", "effigy.scan.attention-markers.v1"),
        load_root_attention_marker_options,
        |options, request| {
            reject_threshold_overrides("attention-markers", request)?;
            apply_common_request_overrides(options, request);
            Ok(())
        },
        run_attention_marker_scan_workspace,
        render_attention_marker_text,
        render_attention_marker_markdown,
    )
}
