use std::path::Path;

use toml::Value;

use crate::DoctorState;

pub(super) fn validate_manifest_schema(
    manifest_path: &Path,
    value: &Value,
    state: &mut DoctorState,
) {
    crate::manifest_schema::validate_manifest_schema(manifest_path, value, state);
}
