mod common;
#[path = "reports/mod.rs"]
mod reports;

pub use reports::{
    render_duplicate_block_markdown, render_duplicate_block_text, render_god_file_markdown,
    render_god_file_text,
};
