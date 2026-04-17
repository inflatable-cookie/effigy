use std::path::Path;

use super::super::scan::execution::run_duplicate_block_scan_workspace;
use super::super::scan::options::doctor_duplicate_block_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;

pub(super) fn check_duplicate_blocks(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_DUPLICATE_BLOCKS,
            label: "duplicate-blocks",
            remediation: remediation::REDUCE_DUPLICATE_BLOCKS,
        },
        doctor_duplicate_block_options,
        run_duplicate_block_scan_workspace,
    );
}
