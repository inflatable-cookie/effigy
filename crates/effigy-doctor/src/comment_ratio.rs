use std::path::Path;

use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use crate::contracts::{check_id, remediation};
use crate::DoctorState;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_comment_ratio_options, run_comment_ratio_scan_workspace};

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
        |root, catalogs| doctor_comment_ratio_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_comment_ratio_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
