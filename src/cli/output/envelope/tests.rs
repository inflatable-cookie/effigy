use super::{build_json_envelope_error_with_warnings, build_json_envelope_success, parse_json_or_string};
use serde_json::json;

#[test]
fn build_json_envelope_success_sets_contract_shape() {
    let payload = build_json_envelope_success("task", "build", json!({"done": true}));
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], true);
    assert_eq!(payload["binary"]["name"], "effigy");
    assert_eq!(
        payload["binary"]["version"],
        effigy_core::build_info::package_version()
    );
    assert_eq!(payload["command"]["kind"], "task");
    assert_eq!(payload["command"]["name"], "build");
    assert_eq!(payload["result"]["done"], true);
    assert_eq!(payload["error"], serde_json::Value::Null);
}

#[test]
fn build_json_envelope_error_sets_contract_shape() {
    let payload = build_json_envelope_error_with_warnings(
        "doctor",
        "doctor",
        "RunnerError",
        "boom",
        Some(json!({"code": 1})),
        &[],
    );
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], false);
    assert_eq!(
        payload["binary"]["display_version"],
        effigy_core::build_info::display_version()
    );
    assert_eq!(payload["command"]["kind"], "doctor");
    assert_eq!(payload["command"]["name"], "doctor");
    assert_eq!(payload["result"], serde_json::Value::Null);
    assert_eq!(payload["error"]["kind"], "RunnerError");
    assert_eq!(payload["error"]["message"], "boom");
    assert_eq!(payload["error"]["details"]["code"], 1);
}

#[test]
fn parse_json_or_string_parses_and_falls_back_to_text() {
    assert_eq!(parse_json_or_string("{\"ok\":true}")["ok"], true);
    assert_eq!(parse_json_or_string("not json")["text"], "not json");
}
