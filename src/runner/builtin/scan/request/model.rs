use std::path::PathBuf;

use effigy_scan::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner::builtin::scan) enum ScanCommand {
    GodFiles,
    DuplicateBlocks,
    CommentRatio,
    GeneratedAssets,
    GeneratedInSrc,
    AttentionMarkers,
    StaleSuppressions,
}

#[derive(Debug)]
pub(in crate::runner::builtin::scan) struct ScanRequest {
    pub(in crate::runner::builtin::scan) command: ScanCommand,
    pub(in crate::runner::builtin::scan) output_json: bool,
    pub(in crate::runner::builtin::scan) format: Option<ScanRenderFormat>,
    pub(in crate::runner::builtin::scan) out: Option<PathBuf>,
    pub(in crate::runner::builtin::scan) warn: Option<usize>,
    pub(in crate::runner::builtin::scan) high: Option<usize>,
    pub(in crate::runner::builtin::scan) critical: Option<usize>,
    pub(in crate::runner::builtin::scan) ratio_warn: Option<f64>,
    pub(in crate::runner::builtin::scan) ratio_high: Option<f64>,
    pub(in crate::runner::builtin::scan) ratio_critical: Option<f64>,
    pub(in crate::runner::builtin::scan) min_code_lines: Option<usize>,
    pub(in crate::runner::builtin::scan) fail_on_findings: bool,
    pub(in crate::runner::builtin::scan) no_gitignore: bool,
    pub(in crate::runner::builtin::scan) show_warnings: bool,
    pub(in crate::runner::builtin::scan) include: Vec<String>,
    pub(in crate::runner::builtin::scan) exclude: Vec<String>,
    pub(in crate::runner::builtin::scan) source_roots: Vec<String>,
    pub(in crate::runner::builtin::scan) warning_markers: Vec<String>,
    pub(in crate::runner::builtin::scan) high_markers: Vec<String>,
    pub(in crate::runner::builtin::scan) critical_markers: Vec<String>,
}
