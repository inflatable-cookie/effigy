use crate::ui::{PlainRenderer, Renderer};
use crate::{emit_json_envelope_success_value, CliExecutionContext};
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
    let version = env!("CARGO_PKG_VERSION");
    let display = format!("effigy v{version}");
    json!({
        "schema": "effigy.version.v1",
        "schema_version": 1,
        "ok": true,
        "binary": "effigy",
        "version": version,
        "display": display,
    })
}

#[cfg(test)]
mod tests {
    use super::build_version_payload;

    #[test]
    fn build_version_payload_sets_schema_and_display() {
        let payload = build_version_payload();
        assert_eq!(payload["schema"], "effigy.version.v1");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["binary"], "effigy");
        assert_eq!(payload["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(
            payload["display"],
            format!("effigy v{}", env!("CARGO_PKG_VERSION"))
        );
    }
}
