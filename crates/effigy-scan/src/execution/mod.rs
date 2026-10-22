use crate::error::ScanError;

mod doctor_inventory;
mod duplicate_blocks;
mod file_scans;
mod workspace;

pub use doctor_inventory::{
    run_doctor_scan_inventory, DoctorScanInventoryOptions, DoctorScanInventoryResult,
};
pub use duplicate_blocks::run_duplicate_block_scan_workspace;
pub use file_scans::run_god_file_scan_workspace;
pub use workspace::{run_workspace_scan, walk_text_scan_files, ScanWorkspaceCounts, WorkspaceScan};
