use serde_json::json;

pub fn build_binary_metadata() -> serde_json::Value {
    // Checkout identity can change between calls. Derive both fields from one
    // observation rather than asking display_version() to inspect Git again.
    let active_version = effigy_core::build_info::active_version();
    let display_version = if active_version.starts_with('v') {
        active_version.clone()
    } else {
        format!("v{active_version}")
    };
    json!({
        "name": "effigy",
        "version": effigy_core::build_info::package_version(),
        "active_version": active_version,
        "display_version": display_version,
    })
}
