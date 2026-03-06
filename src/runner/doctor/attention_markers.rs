use std::path::Path;

use super::super::model::catalog::LoadedCatalog;
use super::super::scan::execution::run_attention_marker_scan_workspace;
use super::super::scan::options::doctor_attention_marker_options;
use super::contracts::{check_id, remediation};
use super::report::DoctorState;
use super::scan_checks::{run_scan_check, ScanDoctorCheck};

pub(super) fn check_attention_markers(
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    state: &mut DoctorState,
) {
    run_scan_check(
        resolved_root,
        catalogs,
        state,
        ScanDoctorCheck {
            check_id: check_id::SCAN_ATTENTION_MARKERS,
            label: "attention-markers",
            remediation: remediation::RESOLVE_ATTENTION_MARKERS,
        },
        doctor_attention_marker_options,
        run_attention_marker_scan_workspace,
    );
}
