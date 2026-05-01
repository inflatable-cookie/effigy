use serde_json::json;

pub fn build_binary_metadata() -> serde_json::Value {
    json!({
        "name": "effigy",
        "version": effigy_core::build_info::package_version(),
        "active_version": effigy_core::build_info::active_version(),
        "display_version": effigy_core::build_info::display_version(),
    })
}
