use std::path::Path;

#[path = "check_registry/definitions.rs"]
mod definitions;
#[path = "check_registry/executor.rs"]
mod executor;

use super::super::{DoctorState, ManifestSnapshot};
use definitions::DoctorCheckContext;

pub(super) fn run_doctor_checks(
    resolved_root: &Path,
    manifest: &ManifestSnapshot,
    state: &mut DoctorState,
) {
    let context = DoctorCheckContext::new(resolved_root, manifest);
    executor::run_registered_checks(definitions::doctor_check_definitions(), &context, state);
}

#[cfg(test)]
mod tests;
