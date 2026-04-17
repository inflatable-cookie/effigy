use std::path::Path;

use super::super::scan::execution::run_god_file_scan_workspace;
use super::super::scan::options::doctor_god_file_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;

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
        doctor_god_file_options,
        run_god_file_scan_workspace,
    );
}
