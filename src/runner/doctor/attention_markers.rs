use std::path::Path;

use super::super::scan::{
    catalog_scan_roots, doctor_attention_marker_options, run_attention_marker_scan_workspace,
    AttentionMarkerSeverity,
};
use super::contracts::{check_id, remediation};
use super::DoctorState;

pub(super) fn check_attention_markers(
    resolved_root: &Path,
    catalogs: &[super::super::LoadedCatalog],
    state: &mut DoctorState,
) {
    let options = match doctor_attention_marker_options(resolved_root, catalogs) {
        Ok(options) => options,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_ATTENTION_MARKERS,
                format!("attention-markers configuration is invalid: {error}"),
                remediation::FIX_MANIFEST_ERRORS_FIRST,
            );
            return;
        }
    };
    if !options.doctor_enabled {
        return;
    }

    let scan_roots = catalog_scan_roots(resolved_root, catalogs);
    let result = match run_attention_marker_scan_workspace(resolved_root, &scan_roots, &options) {
        Ok(result) => result,
        Err(error) => {
            state.add_check_error(
                check_id::SCAN_ATTENTION_MARKERS,
                format!("attention-markers scan failed: {error}"),
                remediation::NO_ACTION_REQUIRED,
            );
            return;
        }
    };

    for finding in result.findings {
        let evidence = format!(
            "{}:{} [{}] {} [{}] {}",
            finding.path,
            finding.line,
            finding.severity.as_str(),
            finding.category.as_str(),
            finding.marker,
            finding.snippet
        );
        match finding.severity {
            AttentionMarkerSeverity::Warning => state.add_check_warning(
                check_id::SCAN_ATTENTION_MARKERS,
                evidence,
                remediation::RESOLVE_ATTENTION_MARKERS,
            ),
            AttentionMarkerSeverity::High | AttentionMarkerSeverity::Critical => {
                state.add_check_error(
                    check_id::SCAN_ATTENTION_MARKERS,
                    evidence,
                    remediation::RESOLVE_ATTENTION_MARKERS,
                )
            }
        }
    }
}
