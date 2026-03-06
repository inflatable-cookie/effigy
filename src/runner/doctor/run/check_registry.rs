use std::path::Path;

#[path = "check_registry/catalog_checks.rs"]
mod catalog_checks;
#[path = "check_registry/definitions.rs"]
mod definitions;
#[path = "check_registry/executor.rs"]
mod executor;
#[path = "check_registry/scan_checks.rs"]
mod scan_checks;

use super::super::progress::DoctorProgressReporter;
use super::super::report::{DoctorState, ManifestSnapshot};
use definitions::DoctorCheckContext;

pub(super) fn run_doctor_checks(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
    progress: Option<&mut DoctorProgressReporter>,
) {
    let context = DoctorCheckContext::new(resolved_root, manifest);
    executor::run_registered_checks(
        definitions::doctor_check_definitions(),
        &context,
        state,
        progress,
    );
}

#[cfg(test)]
mod tests;
