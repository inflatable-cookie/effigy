//! Workspace scan engine for Effigy.
//!
//! Extracted from `src/runner/scan/**` under card `249`. Owns the scan
//! model types, manifest-backed option loading, traversal and heuristic
//! support, execution across the six scan subsystems (god files,
//! duplicate blocks, comment ratio, generated assets, generated-in-src,
//! attention markers, stale suppressions), and the text/markdown
//! renderers.
//!
//! The narrow `ScanError` boundary lifts to `RunnerError` at the
//! runner's edge via `impl From<ScanError> for RunnerError` in
//! `src/runner/error.rs`. Callers import from `effigy_scan::` directly
//! — there is no transitional shim in `src/lib.rs`
//! (242 / second-sweep lesson).
//!
//! `builtin/scan/**` is orchestration over this engine, not a
//! duplicate — see card `244`.

mod constants;
mod error;
mod execution;
mod model;
mod options;
mod render;
mod support;

#[cfg(test)]
mod tests;

pub use error::ScanError;

pub use execution::{
    run_attention_marker_scan_workspace, run_comment_ratio_scan_workspace,
    run_duplicate_block_scan_workspace, run_generated_asset_scan_workspace,
    run_generated_in_src_scan_workspace, run_god_file_scan_workspace,
    run_stale_suppression_scan_workspace, run_workspace_scan, walk_text_scan_files,
    ScanWorkspaceCounts,
};

pub use model::{
    format_bytes, format_ratio, AttentionMarkerCategory, AttentionMarkerFinding,
    AttentionMarkerPatterns, AttentionMarkerScanOptions, AttentionMarkerScanResult,
    AttentionMarkerSeverity, BoundaryLayerRule, BoundaryViolationFinding,
    BoundaryViolationScanOptions, BoundaryViolationScanResult, BoundaryViolationSeverity,
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, CommentRatioSeverity,
    CommentRatioThresholds, DeadCodeConfidence, DeadCodeFinding, DeadCodeFindingKind,
    DeadCodeScanOptions, DeadCodeScanResult, DeadCodeSeverity, DuplicateBlockFinding,
    DuplicateBlockLocation, DuplicateBlockScanOptions, DuplicateBlockScanResult,
    DuplicateBlockSeverity, DuplicateBlockThresholds, GeneratedAssetFinding,
    GeneratedAssetScanOptions, GeneratedAssetScanResult, GeneratedAssetSeverity,
    GeneratedAssetThresholds, GeneratedInSrcCategory, GeneratedInSrcFinding,
    GeneratedInSrcScanOptions, GeneratedInSrcScanResult, GeneratedInSrcSeverity,
    GeneratedInSrcThresholds, GodFileFinding, GodFileScanOptions, GodFileScanResult,
    GodFileSeverity, GodFileThresholds, ScanGraphFileContext, ScanRenderFormat,
    StaleSuppressionCategory, StaleSuppressionFinding, StaleSuppressionPatterns,
    StaleSuppressionScanOptions, StaleSuppressionScanResult, StaleSuppressionSeverity,
    TextRenderOptions, ValidationGapConfidence, ValidationGapFinding, ValidationGapFindingKind,
    ValidationGapScanOptions, ValidationGapScanResult, ValidationGapSeverity,
    ValidationGapTestTarget,
};

pub use options::{
    catalog_scan_roots, doctor_attention_marker_options, doctor_comment_ratio_options,
    doctor_dead_code_options, doctor_duplicate_block_options, doctor_generated_asset_options,
    doctor_generated_in_src_options, doctor_god_file_options, doctor_stale_suppression_options,
    doctor_validation_gap_options, load_root_attention_marker_options,
    load_root_boundary_violation_options, load_root_comment_ratio_options,
    load_root_dead_code_options, load_root_duplicate_block_options,
    load_root_generated_asset_options, load_root_generated_in_src_options,
    load_root_god_file_options, load_root_stale_suppression_options,
    load_root_validation_gap_options,
};

pub use render::{
    render_attention_marker_markdown, render_attention_marker_text,
    render_boundary_violation_markdown, render_boundary_violation_text,
    render_comment_ratio_markdown, render_comment_ratio_text, render_dead_code_markdown,
    render_dead_code_text, render_duplicate_block_markdown, render_duplicate_block_text,
    render_generated_asset_markdown, render_generated_asset_text, render_generated_in_src_markdown,
    render_generated_in_src_text, render_god_file_markdown, render_god_file_text,
    render_stale_suppression_markdown, render_stale_suppression_text,
    render_validation_gap_markdown, render_validation_gap_text,
};
