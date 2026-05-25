use std::path::PathBuf;

use effigy_scan::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::scan) enum ScanCommand {
    GodFiles,
    BoundaryViolations,
    DeadCode,
    ValidationGaps,
    DuplicateBlocks,
    CommentRatio,
    GeneratedAssets,
    GeneratedInSrc,
    AttentionMarkers,
    StaleSuppressions,
}

#[derive(Debug)]
pub(in crate::scan) struct ScanRequest {
    pub(in crate::scan) command: ScanCommand,
    pub(in crate::scan) output_json: bool,
    pub(in crate::scan) graph_context: bool,
    pub(in crate::scan) read_stdin: bool,
    pub(in crate::scan) changed_paths: Vec<String>,
    pub(in crate::scan) format: Option<ScanRenderFormat>,
    pub(in crate::scan) out: Option<PathBuf>,
    pub(in crate::scan) warn: Option<usize>,
    pub(in crate::scan) high: Option<usize>,
    pub(in crate::scan) critical: Option<usize>,
    pub(in crate::scan) ratio_warn: Option<f64>,
    pub(in crate::scan) ratio_high: Option<f64>,
    pub(in crate::scan) ratio_critical: Option<f64>,
    pub(in crate::scan) min_code_lines: Option<usize>,
    pub(in crate::scan) fail_on_findings: bool,
    pub(in crate::scan) no_gitignore: bool,
    pub(in crate::scan) show_warnings: bool,
    pub(in crate::scan) include: Vec<String>,
    pub(in crate::scan) exclude: Vec<String>,
    pub(in crate::scan) source_roots: Vec<String>,
    pub(in crate::scan) warning_markers: Vec<String>,
    pub(in crate::scan) high_markers: Vec<String>,
    pub(in crate::scan) critical_markers: Vec<String>,
}
