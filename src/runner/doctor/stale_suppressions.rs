use std::path::Path;

use super::super::scan::execution::run_stale_suppression_scan_workspace;
use super::super::scan::options::doctor_stale_suppression_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;

pub(super) fn check_stale_suppressions(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_STALE_SUPPRESSIONS,
            label: "stale-suppressions",
            remediation: remediation::REMOVE_STALE_SUPPRESSIONS,
        },
        doctor_stale_suppression_options,
        run_stale_suppression_scan_workspace,
    );
}
