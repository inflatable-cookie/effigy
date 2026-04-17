mod code;
mod generated;
mod markers;
mod severity;
mod strings;

pub use code::{
    comment_ratio_counts, count_code_lines, normalized_code_lines, trim_snippet, NormalizedCodeLine,
};
pub use generated::{generated_asset_reason, generated_in_src_reason, is_generated_artifact};
pub use markers::{
    attention_marker_category, attention_marker_matches_line, compile_attention_marker_patterns,
    compile_stale_suppression_patterns, stale_suppression_category, stale_suppression_matches_line,
};
pub use severity::{
    attention_marker_severity_rank, classify_comment_ratio_severity,
    classify_duplicate_block_severity, classify_generated_asset_severity,
    classify_generated_in_src_severity, classify_severity, comment_ratio_severity_rank,
    duplicate_block_severity_rank, generated_asset_severity_rank, generated_in_src_category_rank,
    generated_in_src_severity_rank, severity_rank, stale_suppression_severity_rank,
};
