use std::path::Path;

use super::super::scan::{
    catalog_scan_roots, doctor_god_file_options, run_god_file_scan_workspace, GodFileSeverity,
};
use super::contracts::{check_id, remediation};
use super::DoctorState;

pub(super) fn check_god_files(
    resolved_root: &Path,
    catalogs: &[super::super::LoadedCatalog],
    state: &mut DoctorState,
) {
    let options = match doctor_god_file_options(resolved_root, catalogs) {
        Ok(options) => options,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_GOD_FILES,
                format!("god-files configuration is invalid: {error}"),
                remediation::FIX_MANIFEST_ERRORS_FIRST,
            );
            return;
        }
    };
    if !options.doctor_enabled {
        return;
    }

    let scan_roots = catalog_scan_roots(resolved_root, catalogs);
    let result = match run_god_file_scan_workspace(resolved_root, &scan_roots, &options) {
        Ok(result) => result,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_GOD_FILES,
                format!("god-files scan failed: {error}"),
                remediation::NO_ACTION_REQUIRED,
            );
            return;
        }
    };

    for finding in result.findings {
        let evidence = format!(
            "{} code lines ({} total) [{}] {}",
            finding.code_lines,
            finding.total_lines,
            finding.severity.as_str(),
            finding.path
        );
        match finding.severity {
            GodFileSeverity::Warning => state.add_check_warning(
                check_id::SCAN_GOD_FILES,
                evidence,
                remediation::SPLIT_GOD_FILES,
            ),
            GodFileSeverity::High | GodFileSeverity::Critical => state.add_check_error(
                check_id::SCAN_GOD_FILES,
                evidence,
                remediation::SPLIT_GOD_FILES,
            ),
        }
    }
}
