mod common;
mod graph;
mod markers;
#[path = "reports/mod.rs"]
mod reports;

pub use graph::{
    render_boundary_violation_markdown, render_boundary_violation_text, render_dead_code_markdown,
    render_dead_code_text, render_validation_gap_markdown, render_validation_gap_text,
};
pub use markers::{
    render_attention_marker_markdown, render_attention_marker_text,
    render_stale_suppression_markdown, render_stale_suppression_text,
};
pub use reports::{
    render_comment_ratio_markdown, render_comment_ratio_text, render_duplicate_block_markdown,
    render_duplicate_block_text, render_generated_asset_markdown, render_generated_asset_text,
    render_generated_in_src_markdown, render_generated_in_src_text, render_god_file_markdown,
    render_god_file_text,
};
