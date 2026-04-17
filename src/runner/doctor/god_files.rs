use std::path::Path;

use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_god_file_options, run_god_file_scan_workspace};

pub(super) fn check_god_files(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_GOD_FILES,
            label: "god-files",
            remediation: remediation::SPLIT_GOD_FILES,
        },
        |root, catalogs| doctor_god_file_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_god_file_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
