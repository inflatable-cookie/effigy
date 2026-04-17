use std::path::Path;

use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use crate::contracts::{check_id, remediation};
use crate::DoctorState;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_generated_in_src_options, run_generated_in_src_scan_workspace};

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
        |root, catalogs| doctor_generated_in_src_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_generated_in_src_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
