//! Workspace scan engine for Effigy.
//!
//! Owns the scan model types, manifest-backed option loading, traversal and
//! heuristic support, execution of the two content scanners that survived the
//! v0.15 scanner cut (`god-files` and `duplicate-blocks`), and the text/markdown
//! renderers.
//!
//! The narrow `ScanError` boundary lifts to `RunnerError` at the runner's edge
//! via `impl From<ScanError> for RunnerError` in `src/runner/error.rs`. Callers
//! import from `effigy_scan::` directly — there is no transitional shim in
//! `src/lib.rs` (242 / second-sweep lesson).
//!
//! `builtin/scan/**` is orchestration over this engine, not a duplicate — see
//! card `244`.

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
    run_doctor_scan_inventory, run_duplicate_block_scan_workspace, run_god_file_scan_workspace,
    run_workspace_scan, walk_text_scan_files, DoctorScanInventoryOptions,
    DoctorScanInventoryResult, ScanWorkspaceCounts,
};

pub use model::{
    god_file_language, DuplicateBlockFinding, DuplicateBlockLocation, DuplicateBlockScanOptions,
    DuplicateBlockScanResult, DuplicateBlockSeverity, DuplicateBlockThresholds, GodFileFinding,
    GodFileScanOptions, GodFileScanResult, GodFileSeverity, GodFileThresholds,
    GodFileThresholdsSource, ScanRenderFormat, TextRenderOptions,
};

pub use options::{
    catalog_scan_roots, doctor_duplicate_block_options, doctor_god_file_options,
    load_root_duplicate_block_options, load_root_god_file_options,
};

pub use render::{
    render_duplicate_block_markdown, render_duplicate_block_text, render_god_file_markdown,
    render_god_file_text,
};
