use std::path::Path;

use super::super::scan::execution::run_comment_ratio_scan_workspace;
use super::super::scan::options::doctor_comment_ratio_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;

pub(super) fn check_comment_ratio(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_COMMENT_RATIO,
            label: "comment-ratio",
            remediation: remediation::REDUCE_COMMENT_RATIO,
        },
        doctor_comment_ratio_options,
        run_comment_ratio_scan_workspace,
    );
}
