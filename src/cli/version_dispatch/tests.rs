use super::build_version_payload;

#[test]
fn build_version_payload_sets_schema_and_display() {
    let payload = build_version_payload();
    assert_eq!(payload["schema"], "effigy.version.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["binary"]["name"], "effigy");
    assert_eq!(
        payload["version"],
        effigy_core::build_info::package_version()
    );
    assert_eq!(
        payload["active_version"],
        effigy_core::build_info::active_version()
    );
    assert_eq!(
        payload["binary"]["display_version"],
        effigy_core::build_info::display_version()
    );
    assert_eq!(
        payload["display"],
        format!("effigy {}", effigy_core::build_info::display_version())
    );
}
