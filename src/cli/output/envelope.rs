use serde_json::json;

use super::build_binary_metadata;

pub fn parse_json_or_string(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({ "text": raw }))
}

pub fn emit_json_envelope_success(kind: &str, name: &str, output: &str) {
    emit_json_envelope_success_value(kind, name, parse_json_or_string(output));
}

pub fn emit_json_envelope_success_value(kind: &str, name: &str, result: serde_json::Value) {
    let payload = build_json_envelope_success(kind, name, result);
    print_json_payload(&payload);
}

/// Emit a success envelope with optional top-level `warnings` metadata
/// (present only when nonempty; spec `116` migration diagnostics).
pub fn emit_json_envelope_success_with_warnings(
    kind: &str,
    name: &str,
    output: &str,
    warnings: &[serde_json::Value],
) {
    emit_json_envelope_success_value_with_warnings(
        kind,
        name,
        parse_json_or_string(output),
        warnings,
    );
}

pub fn emit_json_envelope_success_value_with_warnings(
    kind: &str,
    name: &str,
    result: serde_json::Value,
    warnings: &[serde_json::Value],
) {
    let payload = build_json_envelope_success_with_warnings(kind, name, result, warnings);
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
    emit_json_envelope_error_with_warnings(
        exit_code,
        kind,
        name,
        error_kind,
        message,
        details,
        &[],
    );
}

/// Emit an error envelope with optional top-level `warnings` metadata.
pub fn emit_json_envelope_error_with_warnings(
    exit_code: i32,
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
    warnings: &[serde_json::Value],
) -> ! {
    let payload = build_json_envelope_error_with_warnings(
        kind,
        name,
        error_kind,
        message,
        details,
        warnings,
    );
    print_json_payload(&payload);
    std::process::exit(exit_code);
}

pub fn build_json_envelope_success(
    kind: &str,
    name: &str,
    result: serde_json::Value,
) -> serde_json::Value {
    build_json_envelope_success_with_warnings(kind, name, result, &[])
}

pub fn build_json_envelope_success_with_warnings(
    kind: &str,
    name: &str,
    result: serde_json::Value,
    warnings: &[serde_json::Value],
) -> serde_json::Value {
    let mut payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "binary": build_binary_metadata(),
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": result,
        "error": serde_json::Value::Null,
    });
    attach_warnings(&mut payload, warnings);
    payload
}

pub fn build_json_envelope_error_with_warnings(
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
    warnings: &[serde_json::Value],
) -> serde_json::Value {
    let mut payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": false,
        "binary": build_binary_metadata(),
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
    });
    attach_warnings(&mut payload, warnings);
    payload
}

fn attach_warnings(payload: &mut serde_json::Value, warnings: &[serde_json::Value]) {
    if !warnings.is_empty() {
        payload["warnings"] = serde_json::Value::Array(warnings.to_vec());
    }
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
