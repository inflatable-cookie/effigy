use std::path::Path;

use super::request::{ScanCommand, ScanRequest};
use crate::runner::error::RunnerError;
use effigy_manifest::LoadedCatalog;

#[path = "execution/core/mod.rs"]
mod core;
#[path = "execution/modes.rs"]
mod modes;

pub(super) fn run_scan_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    match request.command {
        ScanCommand::GodFiles => modes::run_god_files(request, target_root, catalogs),
        ScanCommand::DuplicateBlocks => modes::run_duplicate_blocks(request, target_root, catalogs),
        ScanCommand::CommentRatio => modes::run_comment_ratio(request, target_root, catalogs),
        ScanCommand::GeneratedAssets => modes::run_generated_assets(request, target_root, catalogs),
        ScanCommand::GeneratedInSrc => modes::run_generated_in_src(request, target_root, catalogs),
        ScanCommand::AttentionMarkers => {
            modes::run_attention_markers(request, target_root, catalogs)
        }
        ScanCommand::StaleSuppressions => {
            modes::run_stale_suppressions(request, target_root, catalogs)
        }
    }
}
