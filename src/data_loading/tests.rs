use super::*;

#[test]
fn parse_helpers_produce_expected_values() {
    let json = parse_json::<serde_json::Value>(r#"{"ok":true}"#).expect("parse json");
    assert_eq!(
        json.get("ok").and_then(serde_json::Value::as_bool),
        Some(true)
    );

    let toml = parse_toml::<toml::Value>("[tasks]\nping = \"printf ok\"\n").expect("parse toml");
    assert!(toml.get("tasks").is_some());
}
