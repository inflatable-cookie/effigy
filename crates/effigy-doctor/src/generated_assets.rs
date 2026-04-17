use std::path::Path;

use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use crate::contracts::{check_id, remediation};
use crate::DoctorState;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_generated_asset_options, run_generated_asset_scan_workspace};

pub(super) fn check_generated_assets(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_GENERATED_ASSETS,
            label: "generated-assets",
            remediation: remediation::REMOVE_OR_IGNORE_GENERATED_ASSETS,
        },
        |root, catalogs| doctor_generated_asset_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_generated_asset_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
