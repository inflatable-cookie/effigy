use std::path::Path;

use super::request::{ScanCommand, ScanRequest};
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;

#[path = "execution/boundaries.rs"]
mod boundaries;
#[path = "execution/core/mod.rs"]
mod core;
#[path = "execution/dead_code.rs"]
mod dead_code;
#[path = "execution/modes.rs"]
mod modes;
#[path = "execution/validation_gaps.rs"]
mod validation_gaps;

pub(super) fn run_scan_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    match request.command {
        ScanCommand::GodFiles => modes::run_god_files(request, target_root, catalogs),
        ScanCommand::BoundaryViolations => {
            modes::run_boundary_violations(request, target_root, catalogs)
        }
        ScanCommand::DeadCode => modes::run_dead_code(request, target_root, catalogs),
        ScanCommand::ValidationGaps => modes::run_validation_gaps(request, target_root, catalogs),
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
