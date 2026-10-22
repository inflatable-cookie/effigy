#[path = "code_shape.rs"]
mod code_shape;
#[path = "common.rs"]
mod common;

pub use code_shape::{
    god_file_language, DuplicateBlockFinding, DuplicateBlockLocation, DuplicateBlockScanOptions,
    DuplicateBlockScanResult, DuplicateBlockSeverity, DuplicateBlockThresholds, GodFileFinding,
    GodFileScanOptions, GodFileScanResult, GodFileSeverity, GodFileThresholds,
    GodFileThresholdsSource,
};
pub use common::{ScanRenderFormat, TextRenderOptions};
