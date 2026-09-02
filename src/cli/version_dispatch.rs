use crate::{
    build_binary_metadata, emit_json_envelope_success_value_with_warnings, CliExecutionContext,
};
use crate::cli::legacy_direct::{warning_values, LegacyDirectWarning};
use effigy_ui::{PlainRenderer, Renderer};
use serde_json::json;

pub fn run_version_command(
    context: &CliExecutionContext<'_>,
    legacy_direct_warning: Option<&LegacyDirectWarning>,
) {
    let payload = build_version_payload();
    if context.emit_json_envelope {
        emit_json_envelope_success_value_with_warnings(
            context.command_kind,
            context.command_name,
            payload,
            &warning_values(legacy_direct_warning),
        );
        return;
    }
    crate::cli::legacy_direct::print_human_warnings_option(legacy_direct_warning, false);

    let mut renderer = PlainRenderer::stdout(context.output_mode);
    let _ = renderer.text(payload["display"].as_str().unwrap_or_default());
}

pub fn build_version_payload() -> serde_json::Value {
    let binary = build_binary_metadata();
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
