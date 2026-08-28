use std::path::Path;

use super::super::request::ScanRequest;
use super::boundaries::run_boundary_violation_scan;
use super::core::{
    apply_attention_marker_request_overrides, apply_comment_ratio_request_overrides,
    apply_common_request_overrides, apply_generated_in_src_request_overrides,
    apply_stale_suppression_request_overrides, apply_threshold_request_overrides,
    reject_threshold_overrides, run_scan_mode, ScanExecution, ScanModeConfig,
};
use super::dead_code::run_dead_code_scan;
use super::validation_gaps::run_validation_gap_scan;
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{
    catalog_scan_roots, load_root_attention_marker_options, load_root_boundary_violation_options,
    load_root_comment_ratio_options, load_root_dead_code_options,
    load_root_duplicate_block_options, load_root_generated_asset_options,
    load_root_generated_in_src_options, load_root_god_file_options,
    load_root_stale_suppression_options, load_root_validation_gap_options,
    render_attention_marker_markdown, render_attention_marker_text,
    render_boundary_violation_markdown, render_boundary_violation_text,
    render_comment_ratio_markdown, render_comment_ratio_text, render_dead_code_markdown,
    render_dead_code_text, render_duplicate_block_markdown, render_duplicate_block_text,
    render_generated_asset_markdown, render_generated_asset_text, render_generated_in_src_markdown,
    render_generated_in_src_text, render_god_file_markdown, render_god_file_text,
    render_stale_suppression_markdown, render_stale_suppression_text,
    render_validation_gap_markdown, render_validation_gap_text,
    run_attention_marker_scan_workspace, run_comment_ratio_scan_workspace,
    run_duplicate_block_scan_workspace, run_generated_asset_scan_workspace,
    run_generated_in_src_scan_workspace, run_god_file_scan_workspace,
    run_stale_suppression_scan_workspace,
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

pub(super) fn run_boundary_violations(
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
            mode: ScanModeConfig::new("boundary-violations", "effigy.scan.boundary-violations.v1"),
        },
        |root| load_root_boundary_violation_options(root).map_err(Into::into),
        |options, request| {
            reject_threshold_overrides("boundary-violations", request)?;
            apply_common_request_overrides(options, request);
            Ok(())
        },
        |root, _scan_roots, options| run_boundary_violation_scan(root, options),
        render_boundary_violation_text,
        render_boundary_violation_markdown,
    )
}

pub(super) fn run_dead_code(
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
            mode: ScanModeConfig::new("dead-code", "effigy.scan.dead-code.v1"),
        },
        |root| load_root_dead_code_options(root).map_err(Into::into),
        |options, request| {
            reject_threshold_overrides("dead-code", request)?;
            apply_common_request_overrides(options, request);
            Ok(())
        },
        |root, _scan_roots, options| run_dead_code_scan(root, options),
        render_dead_code_text,
        render_dead_code_markdown,
    )
}

pub(super) fn run_validation_gaps(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    let scan_roots = catalog_scan_roots(target_root, catalogs);
    let changed_paths = request.changed_paths.clone();
    let read_stdin = request.read_stdin;
    run_scan_mode(
        ScanExecution {
            request,
            target_root,
            scan_roots: &scan_roots,
            mode: ScanModeConfig::new("validation-gaps", "effigy.scan.validation-gaps.v1"),
        },
        |root| load_root_validation_gap_options(root).map_err(Into::into),
        |options, request| {
            reject_threshold_overrides("validation-gaps", request)?;
            apply_common_request_overrides(options, request);
            Ok(())
        },
        |root, _scan_roots, options| {
            run_validation_gap_scan(root, options, &changed_paths, read_stdin)
        },
        render_validation_gap_text,
        render_validation_gap_markdown,
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

pub(super) fn run_comment_ratio(
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
            mode: ScanModeConfig::new("comment-ratio", "effigy.scan.comment-ratio.v1"),
        },
        |root| load_root_comment_ratio_options(root).map_err(Into::into),
        |options, request| {
            apply_comment_ratio_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_comment_ratio_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_comment_ratio_text,
        render_comment_ratio_markdown,
    )
}

pub(super) fn run_generated_assets(
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
            mode: ScanModeConfig::new("generated-assets", "effigy.scan.generated-assets.v1"),
        },
        |root| load_root_generated_asset_options(root).map_err(Into::into),
        |options, request| {
            apply_threshold_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_generated_asset_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_generated_asset_text,
        render_generated_asset_markdown,
    )
}

pub(super) fn run_generated_in_src(
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
            mode: ScanModeConfig::new("generated-in-src", "effigy.scan.generated-in-src.v1"),
        },
        |root| load_root_generated_in_src_options(root).map_err(Into::into),
        |options, request| {
            apply_generated_in_src_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_generated_in_src_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_generated_in_src_text,
        render_generated_in_src_markdown,
    )
}

pub(super) fn run_attention_markers(
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
            mode: ScanModeConfig::new("attention-markers", "effigy.scan.attention-markers.v1"),
        },
        |root| load_root_attention_marker_options(root).map_err(Into::into),
        |options, request| {
            reject_threshold_overrides("attention-markers", request)?;
            apply_attention_marker_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_attention_marker_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_attention_marker_text,
        render_attention_marker_markdown,
    )
}

pub(super) fn run_stale_suppressions(
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
            mode: ScanModeConfig::new("stale-suppressions", "effigy.scan.stale-suppressions.v1"),
        },
        |root| load_root_stale_suppression_options(root).map_err(Into::into),
        |options, request| {
            reject_threshold_overrides("stale-suppressions", request)?;
            apply_stale_suppression_request_overrides(options, request);
            Ok(())
        },
        |root, scan_roots, options| {
            run_stale_suppression_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
        render_stale_suppression_text,
        render_stale_suppression_markdown,
    )
}
