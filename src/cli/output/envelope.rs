use serde_json::json;

use super::build_binary_metadata;

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

pub(crate) fn emit_json_envelope_success_with_binary(
    kind: &str,
    name: &str,
    result: serde_json::Value,
    binary: serde_json::Value,
) {
    let payload = build_json_envelope_success_with_binary(kind, name, result, binary);
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

/// The same refusal, carried in both forms: `message` and `details` stay the
/// machine contract, and `error.human` adds the three facts a person reads
/// (what failed, the evidence, the next action).
pub fn emit_json_envelope_error_with_human(
    exit_code: i32,
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
    human: serde_json::Value,
) -> ! {
    let payload =
        build_json_envelope_error_with_human(kind, name, error_kind, message, details, human);
    print_json_payload(&payload);
    std::process::exit(exit_code);
}

pub fn build_json_envelope_error_with_human(
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
    human: serde_json::Value,
) -> serde_json::Value {
    let mut payload = build_json_envelope_error(kind, name, error_kind, message, details);
    if let Some(error) = payload.get_mut("error").and_then(|error| error.as_object_mut()) {
        error.insert("human".to_owned(), human);
    }
    payload
}

pub fn build_json_envelope_success(
    kind: &str,
    name: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    build_json_envelope_success_with_binary(kind, name, result, build_binary_metadata())
}

fn build_json_envelope_success_with_binary(
    kind: &str,
    name: &str,
    result: serde_json::Value,
    binary: serde_json::Value,
) -> serde_json::Value {
    json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "binary": binary,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": result,
        "warnings": retired_config_warnings(),
        "error": serde_json::Value::Null,
    })
}

/// Retired v0.15 config this invocation read, as envelope warnings. JSON mode
/// keeps them out of stderr so machine readers get one parseable document.
fn retired_config_warnings() -> serde_json::Value {
    serde_json::Value::Array(
        effigy_manifest::take_retired_notices()
            .into_iter()
            .map(serde_json::Value::String)
            .collect(),
    )
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
        "binary": build_binary_metadata(),
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": serde_json::Value::Null,
        "warnings": retired_config_warnings(),
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
