use crate::{
    emit_json_envelope_success_value, help_topic_label, render_cli_header, CliExecutionContext,
};
use effigy_cli::help::ui::render_help_with_deferred_builtins;
use effigy_cli::HelpTopic;
use effigy_ui::{PlainRenderer, Renderer};
use serde_json::json;

pub fn run_help_command(context: &CliExecutionContext<'_>, topic: HelpTopic) {
    let deferred_builtins = crate::runner::deferred_builtins_for_root(context.command_root);
    if context.suppress_header {
        let payload = build_help_payload_for_root(topic, context.command_root);
        if context.emit_json_envelope {
            emit_json_envelope_success_value(context.command_kind, context.command_name, payload);
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

    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !context.suppress_header {
        let _ = render_cli_header(&mut renderer, context.command_root);
    }
    let _ = render_help_with_deferred_builtins(&mut renderer, topic, &deferred_builtins);
    let _ = renderer.text("");
}

pub fn build_help_payload(topic: HelpTopic) -> serde_json::Value {
    build_help_payload_for_root(topic, &std::env::current_dir().unwrap_or_default())
}

pub fn build_help_payload_for_root(topic: HelpTopic, root: &std::path::Path) -> serde_json::Value {
    let topic_label = help_topic_label(topic);
    let mut help_renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    let deferred_builtins = crate::runner::deferred_builtins_for_root(root);
    let _ = render_help_with_deferred_builtins(&mut help_renderer, topic, &deferred_builtins);
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
#[path = "help_dispatch/tests.rs"]
mod tests;
