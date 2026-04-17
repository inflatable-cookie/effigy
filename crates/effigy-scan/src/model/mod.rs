#[path = "code_shape.rs"]
mod code_shape;
#[path = "common.rs"]
mod common;
#[path = "generated.rs"]
mod generated;
#[path = "markers.rs"]
mod markers;

pub use code_shape::{
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, CommentRatioSeverity,
    CommentRatioThresholds, DuplicateBlockFinding, DuplicateBlockLocation,
    DuplicateBlockScanOptions, DuplicateBlockScanResult, DuplicateBlockSeverity,
    DuplicateBlockThresholds, GodFileFinding, GodFileScanOptions, GodFileScanResult,
    GodFileSeverity, GodFileThresholds,
};
pub use common::{format_bytes, format_ratio, ScanRenderFormat, TextRenderOptions};
pub use generated::{
    GeneratedAssetFinding, GeneratedAssetScanOptions, GeneratedAssetScanResult,
    GeneratedAssetSeverity, GeneratedAssetThresholds, GeneratedInSrcCategory,
    GeneratedInSrcFinding, GeneratedInSrcScanOptions, GeneratedInSrcScanResult,
    GeneratedInSrcSeverity, GeneratedInSrcThresholds,
};
pub use markers::{
    AttentionMarkerCategory, AttentionMarkerFinding, AttentionMarkerPatterns,
    AttentionMarkerScanOptions, AttentionMarkerScanResult, AttentionMarkerSeverity,
    StaleSuppressionCategory, StaleSuppressionFinding, StaleSuppressionPatterns,
    StaleSuppressionScanOptions, StaleSuppressionScanResult, StaleSuppressionSeverity,
};
