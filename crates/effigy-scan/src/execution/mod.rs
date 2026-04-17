use crate::error::ScanError;

mod duplicate_blocks;
mod file_scans;
mod marker_scans;
mod workspace;

pub use duplicate_blocks::run_duplicate_block_scan_workspace;
pub use file_scans::{
    run_comment_ratio_scan_workspace, run_generated_asset_scan_workspace,
    run_generated_in_src_scan_workspace, run_god_file_scan_workspace,
};
pub use marker_scans::{run_attention_marker_scan_workspace, run_stale_suppression_scan_workspace};
pub use workspace::{run_workspace_scan, walk_text_scan_files, ScanWorkspaceCounts};
