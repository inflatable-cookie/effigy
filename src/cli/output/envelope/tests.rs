use super::{
    build_json_envelope_error, build_json_envelope_error_with_human, build_json_envelope_success,
    parse_json_or_string,
};
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
    let payload = build_json_envelope_error(
        "doctor",
        "doctor",
        "RunnerError",
        "boom",
        Some(json!({"code": 1})),
    );
    assert_eq!(payload["schema"], "effigy.command.v1");
    assert_eq!(payload["schema_version"], 1);
    assert_eq!(payload["ok"], false);
    let display_version = payload["binary"]["display_version"]
        .as_str()
        .expect("display version string");
    assert!(!display_version.is_empty());
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

#[test]
fn build_json_envelope_error_with_human_keeps_the_machine_fields() {
    let human = effigy_ui::HumanError::new("container `web` never became healthy")
        .evidence("health check answered HTTP 404 (attempt 12, 60s)")
        .next_action("run `effigy container web logs`")
        .to_json();
    let payload = build_json_envelope_error_with_human(
        "container",
        "up",
        "RunnerError",
        "container `web` never became healthy",
        Some(json!({"schema": "effigy.container.up.v1"})),
        human,
    );
    assert_eq!(payload["ok"], false);
    assert_eq!(payload["error"]["kind"], "RunnerError");
    assert_eq!(
        payload["error"]["message"],
        "container `web` never became healthy"
    );
    assert_eq!(
        payload["error"]["details"]["schema"],
        "effigy.container.up.v1"
    );
    assert_eq!(
        payload["error"]["human"]["what_failed"],
        "container `web` never became healthy"
    );
    assert_eq!(
        payload["error"]["human"]["evidence"][0],
        "health check answered HTTP 404 (attempt 12, 60s)"
    );
    assert_eq!(
        payload["error"]["human"]["next_action"],
        "run `effigy container web logs`"
    );
}
