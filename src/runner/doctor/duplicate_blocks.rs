use std::path::Path;

use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_duplicate_block_options, run_duplicate_block_scan_workspace};

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
        |root, catalogs| doctor_duplicate_block_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_duplicate_block_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
