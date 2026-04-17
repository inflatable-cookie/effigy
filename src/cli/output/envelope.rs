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
#[path = "envelope/tests.rs"]
mod tests;
