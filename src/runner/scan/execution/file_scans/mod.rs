use std::path::{Path, PathBuf};

use super::super::model::{
    CommentRatioFinding, CommentRatioScanOptions, CommentRatioScanResult, GeneratedAssetFinding,
    GeneratedAssetScanOptions, GeneratedAssetScanResult, GeneratedInSrcFinding,
    GeneratedInSrcScanOptions, GeneratedInSrcScanResult, GodFileFinding, GodFileScanOptions,
    GodFileScanResult,
};
use super::super::support::{
    classify_comment_ratio_severity, classify_generated_asset_severity,
    classify_generated_in_src_severity, classify_severity, comment_ratio_counts,
    comment_ratio_severity_rank, compile_glob_set, count_code_lines, generated_asset_reason,
    generated_asset_severity_rank, generated_in_src_category_rank, generated_in_src_reason,
    generated_in_src_severity_rank, is_generated_artifact, read_asset_sample, rebase_finding_path,
    severity_rank, should_skip_generated_asset_path, should_skip_path, walk_scan_files,
};
use super::{run_workspace_scan, walk_text_scan_files, RunnerError, ScanWorkspaceCounts};

mod code_shape;
mod generated;

pub(in crate::runner) use code_shape::{
    run_comment_ratio_scan_workspace, run_god_file_scan_workspace,
};
pub(in crate::runner) use generated::{
    run_generated_asset_scan_workspace, run_generated_in_src_scan_workspace,
};
