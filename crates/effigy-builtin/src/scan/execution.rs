use std::path::Path;

use super::request::{ScanCommand, ScanRequest};
use crate::BuiltinError;
use effigy_manifest::LoadedCatalog;

#[path = "execution/core/mod.rs"]
mod core;
#[path = "execution/modes.rs"]
mod modes;

pub(super) fn run_scan_request(
    request: ScanRequest,
    target_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, BuiltinError> {
    match request.command {
        ScanCommand::GodFiles => modes::run_god_files(request, target_root, catalogs),
        ScanCommand::DuplicateBlocks => modes::run_duplicate_blocks(request, target_root, catalogs),
    }
}
