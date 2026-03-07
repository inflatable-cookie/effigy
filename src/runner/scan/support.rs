use super::constants::{
    DEFAULT_DATA_DIRS, DEFAULT_DOC_DIRS, DEFAULT_EXCLUDED_DIRS, DEFAULT_LOCK_FILE_NAMES,
    GENERATED_ASSET_DIRS, GENERATED_ASSET_NAME_MARKERS, GENERATED_MARKERS,
};
use crate::runner::error::RunnerError;

#[path = "support/heuristics/mod.rs"]
mod heuristics;
#[path = "support/traversal.rs"]
mod traversal;

pub(in crate::runner) use heuristics::{
    attention_marker_category, attention_marker_matches_line, attention_marker_severity_rank,
    classify_comment_ratio_severity, classify_duplicate_block_severity,
    classify_generated_asset_severity, classify_generated_in_src_severity, classify_severity,
    comment_ratio_counts, comment_ratio_severity_rank, compile_attention_marker_patterns,
    compile_stale_suppression_patterns, count_code_lines, duplicate_block_severity_rank,
    generated_asset_reason, generated_asset_severity_rank, generated_in_src_category_rank,
    generated_in_src_reason, generated_in_src_severity_rank, is_generated_artifact,
    normalized_code_lines, severity_rank, stale_suppression_category,
    stale_suppression_matches_line, stale_suppression_severity_rank, trim_snippet,
    NormalizedCodeLine,
};
#[cfg(test)]
pub(in crate::runner) use traversal::normalize_rel_path;
pub(in crate::runner) use traversal::{
    compile_glob_set, read_asset_sample, rebase_finding_path, should_skip_generated_asset_path,
    should_skip_path, walk_scan_files, workspace_scan_roots,
};
