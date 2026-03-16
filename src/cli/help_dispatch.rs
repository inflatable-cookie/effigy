use crate::ui::{PlainRenderer, Renderer};
use crate::{
    emit_json_envelope_success_value, help_topic_label, render_cli_header,
    render_help_with_deferred_builtins, CliExecutionContext, HelpTopic,
};
use serde_json::json;

pub fn run_help_command(context: &CliExecutionContext<'_>, topic: HelpTopic) {
    let deferred_builtins =
        crate::runner::explicitly_deferred_builtins_for_root(context.command_root);
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
    let deferred_builtins = crate::runner::explicitly_deferred_builtins_for_root(root);
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
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{build_help_payload, build_help_payload_for_root};
    use crate::HelpTopic;

    fn temp_workspace(name: &str) -> std::path::PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("effigy-help-{name}-{ts}"));
        fs::create_dir_all(&root).expect("mkdir workspace");
        fs::write(root.join("package.json"), "{}\n").expect("write package marker");
        root
    }

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

    #[test]
    fn build_help_payload_for_root_hides_explicitly_deferred_builtins() {
        let root = temp_workspace("help-hidden-deferred-builtin");
        fs::write(
            root.join("effigy.toml"),
            "[defer]\nrun = \"printf deferred\"\nbuiltins = [\"release\"]\n",
        )
        .expect("write manifest");

        let payload = build_help_payload_for_root(HelpTopic::General, &root);
        let text = payload["text"].as_str().expect("help text");
        assert!(!text.contains("effigy release"), "got: {text}");
        assert!(text.contains("effigy doctor"), "got: {text}");
    }
}
