use super::definitions::DoctorCheckContext;
use crate::contracts::{check_id, remediation};
use crate::DoctorState;

/// Reports pinned `[sources.<name>]` drift. Nothing is repaired: a working copy
/// with local changes or a different HEAD stays exactly as the developer left
/// it (contract 020).
pub(super) fn run_pinned_source_drift_check(
    context: &DoctorCheckContext<'_>,
    state: &mut DoctorState,
) {
    let manifest_path = context
        .resolved_root
        .join(effigy_manifest::TASK_MANIFEST_FILE);
    if !manifest_path.is_file() {
        return;
    }
    let Ok(reports) = effigy_manifest::inspect_pinned_sources(&manifest_path) else {
        // A manifest that cannot be read is reported by the parse/schema checks.
        return;
    };
    if reports.is_empty() {
        return;
    }
    let mut clean = Vec::new();
    for report in &reports {
        match report.drift_message() {
            Some(message) => state.add_check_error(
                check_id::PINNED_SOURCES_DRIFT,
                message,
                remediation::RESOLVE_PINNED_SOURCE_DRIFT,
            ),
            None => clean.push(format!(
                "{} at {} ({})",
                report.name,
                report.path.display(),
                report.state.as_str()
            )),
        }
    }
    if !clean.is_empty() {
        state.add_check_info(
            check_id::PINNED_SOURCES_DRIFT,
            format!("pinned sources agree with their manifests: {}", clean.join(", ")),
            remediation::NO_ACTION_REQUIRED,
        );
    }
}
