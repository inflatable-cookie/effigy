use std::path::Path;

use super::super::scan::{
    catalog_scan_roots, doctor_generated_asset_options, run_generated_asset_scan_workspace,
    GeneratedAssetSeverity,
};
use super::contracts::{check_id, remediation};
use super::DoctorState;

pub(super) fn check_generated_assets(
    resolved_root: &Path,
    catalogs: &[super::super::LoadedCatalog],
    state: &mut DoctorState,
) {
    let options = match doctor_generated_asset_options(resolved_root, catalogs) {
        Ok(options) => options,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_GENERATED_ASSETS,
                format!("generated-assets configuration is invalid: {error}"),
                remediation::FIX_MANIFEST_ERRORS_FIRST,
            );
            return;
        }
    };
    if !options.doctor_enabled {
        return;
    }

    let scan_roots = catalog_scan_roots(resolved_root, catalogs);
    let result = match run_generated_asset_scan_workspace(resolved_root, &scan_roots, &options) {
        Ok(result) => result,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_GENERATED_ASSETS,
                format!("generated-assets scan failed: {error}"),
                remediation::NO_ACTION_REQUIRED,
            );
            return;
        }
    };

    for finding in result.findings {
        let evidence = format!(
            "{} [{}] {} ({})",
            super::super::scan::format_bytes(finding.bytes),
            finding.severity.as_str(),
            finding.path,
            finding.reason
        );
        match finding.severity {
            GeneratedAssetSeverity::Warning => state.add_check_warning(
                check_id::SCAN_GENERATED_ASSETS,
                evidence,
                remediation::REMOVE_OR_IGNORE_GENERATED_ASSETS,
            ),
            GeneratedAssetSeverity::High | GeneratedAssetSeverity::Critical => state.add_check_error(
                check_id::SCAN_GENERATED_ASSETS,
                evidence,
                remediation::REMOVE_OR_IGNORE_GENERATED_ASSETS,
            ),
        }
    }
}
