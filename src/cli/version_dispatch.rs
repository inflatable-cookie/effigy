use crate::cli::output::emit_json_envelope_success_with_binary;
use crate::{build_binary_metadata, CliExecutionContext};
use effigy_ui::{PlainRenderer, Renderer};
use serde_json::json;

pub fn run_version_command(context: &CliExecutionContext<'_>) {
    let binary = build_binary_metadata();
    let payload = build_version_payload_with_binary(binary.clone());
    if context.emit_json_envelope {
        emit_json_envelope_success_with_binary(
            context.command_kind,
            context.command_name,
            payload,
            binary,
        );
        return;
    }

    let mut renderer = PlainRenderer::stdout(context.output_mode);
    let _ = renderer.text(payload["display"].as_str().unwrap_or_default());
}

pub fn build_version_payload() -> serde_json::Value {
    build_version_payload_with_binary(build_binary_metadata())
}

fn build_version_payload_with_binary(binary: serde_json::Value) -> serde_json::Value {
    let version = binary["version"].as_str().unwrap_or_default();
    let active_version = binary["active_version"].as_str().unwrap_or_default();
    let display = format!(
        "effigy {}",
        binary["display_version"].as_str().unwrap_or_default()
    );
    json!({
        "schema": "effigy.version.v1",
        "schema_version": 1,
        "ok": true,
        "binary": binary,
        "version": version,
        "active_version": active_version,
        "display": display,
    })
}

#[cfg(test)]
#[path = "version_dispatch/tests.rs"]
mod tests;
