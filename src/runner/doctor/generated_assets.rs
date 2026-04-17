use std::path::Path;

use super::super::scan::execution::run_generated_asset_scan_workspace;
use super::super::scan::options::doctor_generated_asset_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use effigy_manifest::LoadedCatalog;

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
        doctor_generated_asset_options,
        run_generated_asset_scan_workspace,
    );
}
