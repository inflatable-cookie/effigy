use crate::{emit_json_envelope_success_value, CliExecutionContext};
use effigy_ui::{PlainRenderer, Renderer};
use serde_json::json;

pub fn run_version_command(context: &CliExecutionContext<'_>) {
    let payload = build_version_payload();
    if context.emit_json_envelope {
        emit_json_envelope_success_value(context.command_kind, context.command_name, payload);
        return;
    }

    let mut renderer = PlainRenderer::stdout(context.output_mode);
    let _ = renderer.text(payload["display"].as_str().unwrap_or_default());
}

pub fn build_version_payload() -> serde_json::Value {
    let version = effigy_core::build_info::package_version();
    let active_version = effigy_core::build_info::active_version();
    let display = format!("effigy {}", effigy_core::build_info::display_version());
    json!({
        "schema": "effigy.version.v1",
        "schema_version": 1,
        "ok": true,
        "binary": "effigy",
        "version": version,
        "active_version": active_version,
        "display": display,
    })
}

#[cfg(test)]
#[path = "version_dispatch/tests.rs"]
mod tests;
