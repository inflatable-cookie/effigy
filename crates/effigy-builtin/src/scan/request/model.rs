use std::path::PathBuf;

use effigy_scan::ScanRenderFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::scan) enum ScanCommand {
    GodFiles,
    DuplicateBlocks,
}

#[derive(Debug)]
pub(in crate::scan) struct ScanRequest {
    pub(in crate::scan) command: ScanCommand,
    pub(in crate::scan) output_json: bool,
    pub(in crate::scan) format: Option<ScanRenderFormat>,
    pub(in crate::scan) out: Option<PathBuf>,
    pub(in crate::scan) warn: Option<usize>,
    pub(in crate::scan) high: Option<usize>,
    pub(in crate::scan) critical: Option<usize>,
    pub(in crate::scan) fail_on_findings: bool,
    pub(in crate::scan) no_gitignore: bool,
    pub(in crate::scan) show_warnings: bool,
    pub(in crate::scan) include: Vec<String>,
    pub(in crate::scan) exclude: Vec<String>,
}
