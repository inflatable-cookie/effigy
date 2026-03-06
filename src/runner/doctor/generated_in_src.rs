use std::path::Path;

use super::super::model::catalog::LoadedCatalog;
use super::super::scan::execution::run_generated_in_src_scan_workspace;
use super::super::scan::options::doctor_generated_in_src_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};

pub(super) fn check_generated_in_src(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_GENERATED_IN_SRC,
            label: "generated-in-src",
            remediation: remediation::REMOVE_GENERATED_FROM_SOURCE_TREES,
        },
        doctor_generated_in_src_options,
        run_generated_in_src_scan_workspace,
    );
}
