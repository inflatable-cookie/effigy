mod code_shape;
mod size_and_paths;

pub use code_shape::{
    render_comment_ratio_markdown, render_comment_ratio_text, render_duplicate_block_markdown,
    render_duplicate_block_text, render_god_file_markdown, render_god_file_text,
};
pub use size_and_paths::{
    render_generated_asset_markdown, render_generated_asset_text, render_generated_in_src_markdown,
    render_generated_in_src_text,
};
