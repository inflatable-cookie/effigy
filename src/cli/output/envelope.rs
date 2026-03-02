use serde_json::json;

pub fn parse_json_or_string(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({ "text": raw }))
}

pub fn emit_json_envelope_success(kind: &str, name: &str, output: &str) {
    let result = parse_json_or_string(output);
    emit_json_envelope_success_value(kind, name, result);
}

pub fn emit_json_envelope_success_value(kind: &str, name: &str, result: serde_json::Value) {
    let payload = build_json_envelope_success(kind, name, result);
    print_json_payload(&payload);
}

pub fn emit_json_envelope_error(
    exit_code: i32,
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> ! {
    let payload = build_json_envelope_error(kind, name, error_kind, message, details);
    print_json_payload(&payload);
    std::process::exit(exit_code);
}

pub fn build_json_envelope_success(
    kind: &str,
    name: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": result,
        "error": serde_json::Value::Null,
    })
}

pub fn build_json_envelope_error(
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> serde_json::Value {
    json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": false,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": serde_json::Value::Null,
        "error": {
            "kind": error_kind,
            "message": message,
            "details": details.unwrap_or(serde_json::Value::Null),
        }
    })
}

fn print_json_payload(payload: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"kind\":\"JsonEncodeError\"}}".to_owned()
        })
    );
}

#[cfg(test)]
mod tests {
    use super::{build_json_envelope_error, build_json_envelope_success, parse_json_or_string};
    use serde_json::json;

    #[test]
    fn build_json_envelope_success_sets_contract_shape() {
        let payload = build_json_envelope_success("task", "build", json!({"done": true}));
        assert_eq!(payload["schema"], "effigy.command.v1");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["ok"], true);
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
}
