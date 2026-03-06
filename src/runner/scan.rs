use super::{
    manifest::{ManifestAttentionMarkersConfig, ManifestGeneratedAssetsConfig},
    LoadedCatalog, ManifestGodFilesConfig, ManifestScanOutputFormat, RunnerError, TaskManifest,
    TASK_MANIFEST_FILE,
};

#[path = "scan/constants.rs"]
mod constants;
#[path = "scan/execution.rs"]
mod execution;
#[path = "scan/model.rs"]
mod model;
#[path = "scan/options.rs"]
mod options;
#[path = "scan/render.rs"]
mod render;
#[path = "scan/support.rs"]
mod support;

use constants::{
    DEFAULT_ATTENTION_MARKER_CRITICAL, DEFAULT_ATTENTION_MARKER_HIGH,
    DEFAULT_ATTENTION_MARKER_WARNING, DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS,
    DEFAULT_GENERATED_ASSETS_CRITICAL_BYTES, DEFAULT_GENERATED_ASSETS_HIGH_BYTES,
    DEFAULT_GENERATED_ASSETS_WARN_BYTES, DEFAULT_GOD_FILES_CRITICAL, DEFAULT_GOD_FILES_HIGH,
    DEFAULT_GOD_FILES_WARN, DEFAULT_LOCK_FILE_NAMES, GENERATED_ASSET_DIRS,
    GENERATED_ASSET_NAME_MARKERS, GENERATED_MARKERS,
};
pub(in crate::runner) use execution::{
    run_attention_marker_scan_workspace, run_generated_asset_scan_workspace,
    run_god_file_scan_workspace,
};
pub(in crate::runner) use model::{
    format_bytes, AttentionMarkerCategory, AttentionMarkerFinding, AttentionMarkerPatterns,
    AttentionMarkerScanOptions, AttentionMarkerScanResult, AttentionMarkerSeverity,
    GeneratedAssetFinding, GeneratedAssetScanOptions, GeneratedAssetScanResult,
    GeneratedAssetSeverity, GeneratedAssetThresholds, GodFileFinding, GodFileScanOptions,
    GodFileScanResult, GodFileSeverity, GodFileThresholds, ScanRenderFormat, TextRenderOptions,
};
pub(in crate::runner) use options::{
    catalog_scan_roots, doctor_attention_marker_options, doctor_generated_asset_options,
    doctor_god_file_options, load_root_attention_marker_options, load_root_generated_asset_options,
    load_root_god_file_options,
};
pub(in crate::runner) use render::{
    render_attention_marker_markdown, render_attention_marker_text,
    render_generated_asset_markdown, render_generated_asset_text, render_god_file_markdown,
    render_god_file_text,
};

#[cfg(test)]
#[path = "scan/tests.rs"]
mod tests;
