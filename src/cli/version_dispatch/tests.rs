use super::build_version_payload;

#[test]
fn build_version_payload_sets_schema_and_display() {
    let payload = build_version_payload();
    assert_eq!(payload["schema"], "effigy.version.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["binary"], "effigy");
    assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(
        payload["display"],
        format!("effigy v{}", env!("CARGO_PKG_VERSION"))
    );
}
