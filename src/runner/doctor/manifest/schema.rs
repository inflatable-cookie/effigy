use std::path::Path;

use toml::Value;

use super::super::report::DoctorState;

pub(super) fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    state: &mut DoctorState,
) {
    effigy_doctor::manifest_schema::validate_manifest_schema(manifest_path, value, state);
}
