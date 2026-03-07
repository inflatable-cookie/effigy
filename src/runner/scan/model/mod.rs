#[path = "code_shape.rs"]
mod code_shape;
#[path = "common.rs"]
mod common;
#[path = "generated.rs"]
mod generated;
#[path = "markers.rs"]
mod markers;

pub(in crate::runner) use code_shape::{
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, CommentRatioSeverity,
    CommentRatioThresholds, DuplicateBlockFinding, DuplicateBlockLocation,
    DuplicateBlockScanOptions, DuplicateBlockScanResult, DuplicateBlockSeverity,
    DuplicateBlockThresholds, GodFileFinding, GodFileScanOptions, GodFileScanResult,
    GodFileSeverity, GodFileThresholds,
};
pub(in crate::runner) use common::{
    format_bytes, format_ratio, ScanRenderFormat, TextRenderOptions,
};
pub(in crate::runner) use generated::{
    GeneratedAssetFinding, GeneratedAssetScanOptions, GeneratedAssetScanResult,
    GeneratedAssetSeverity, GeneratedAssetThresholds, GeneratedInSrcCategory,
    GeneratedInSrcFinding, GeneratedInSrcScanOptions, GeneratedInSrcScanResult,
    GeneratedInSrcSeverity, GeneratedInSrcThresholds,
};
pub(in crate::runner) use markers::{
    AttentionMarkerCategory, AttentionMarkerFinding, AttentionMarkerPatterns,
    AttentionMarkerScanOptions, AttentionMarkerScanResult, AttentionMarkerSeverity,
    StaleSuppressionCategory, StaleSuppressionFinding, StaleSuppressionPatterns,
    StaleSuppressionScanOptions, StaleSuppressionScanResult, StaleSuppressionSeverity,
};
