use std::path::Path;

use crate::ui::{OutputMode, PlainRenderer, Renderer};
use crate::{
    emit_json_envelope_success_value, help_topic_label, render_cli_header, render_help, HelpTopic,
};
use serde_json::json;

pub fn run_help_command(
    output_mode: OutputMode,
    command_root: &Path,
    suppress_header: bool,
    emit_json_envelope: bool,
    command_kind: &str,
    command_name: &str,
    topic: HelpTopic,
) {
    if suppress_header {
        let payload = build_help_payload(topic);
        if emit_json_envelope {
            emit_json_envelope_success_value(command_kind, command_name, payload);
            return;
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| {
                "{\"ok\":false,\"error\":{\"kind\":\"JsonEncodeError\"}}".to_owned()
            })
        );
        return;
    }

    let mut renderer = PlainRenderer::stdout(output_mode);
    if !suppress_header {
        let _ = render_cli_header(&mut renderer, command_root);
    }
    let _ = render_help(&mut renderer, topic);
    let _ = renderer.text("");
}

pub fn build_help_payload(topic: HelpTopic) -> serde_json::Value {
    let topic_label = help_topic_label(topic);
    let mut help_renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    let _ = render_help(&mut help_renderer, topic);
    let rendered = String::from_utf8(help_renderer.into_inner()).unwrap_or_default();
    json!({
        "schema": "effigy.help.v1",
        "schema_version": 1,
        "ok": true,
        "topic": topic_label,
        "text": rendered,
    })
}

#[cfg(test)]
mod tests {
    use super::build_help_payload;
    use crate::HelpTopic;

    #[test]
    fn build_help_payload_sets_schema_and_topic() {
        let payload = build_help_payload(HelpTopic::Doctor);
        assert_eq!(payload["schema"], "effigy.help.v1");
        assert_eq!(payload["schema_version"], 1);
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["topic"], "doctor");
        assert!(payload["text"]
            .as_str()
            .is_some_and(|text| text.contains("doctor")));
    }
}
