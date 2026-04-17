use std::path::Path;

use super::scan_checks::{run_scan_check, ScanDoctorCheck};
use crate::contracts::{check_id, remediation};
use crate::DoctorState;
use effigy_manifest::LoadedCatalog;
use effigy_scan::{doctor_attention_marker_options, run_attention_marker_scan_workspace};

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
        |root, catalogs| doctor_attention_marker_options(root, catalogs).map_err(Into::into),
        |root, scan_roots, options| {
            run_attention_marker_scan_workspace(root, scan_roots, options).map_err(Into::into)
        },
    );
}
