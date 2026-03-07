mod comment_ratio;
mod duplicate_blocks;
mod god_files;

pub(in crate::runner) use comment_ratio::{
    render_comment_ratio_markdown, render_comment_ratio_text,
};
pub(in crate::runner) use duplicate_blocks::{
    render_duplicate_block_markdown, render_duplicate_block_text,
};
pub(in crate::runner) use god_files::{render_god_file_markdown, render_god_file_text};
