use effigy::ui::{OutputMode, PlainRenderer, Renderer};
use effigy::{
    apply_global_json_flag, command_kind_and_name, command_requests_json, emit_json_envelope_error,
    emit_json_envelope_success_value, help_topic_label, parse_command, parse_error_json_details,
    render_cli_header, render_help, render_parse_error, run_and_render_command,
    strip_global_json_flags, Command,
};
use serde_json::json;

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (args, global_json_mode) = strip_global_json_flags(raw_args);
    let output_mode = OutputMode::from_env();
    let parsed = match parse_command(args) {
        Ok(cmd) => cmd,
        Err(err) => {
            if global_json_mode {
                emit_json_envelope_error(
                    2,
                    "cli",
                    "parse",
                    "CliParseError",
                    &err.to_string(),
                    Some(parse_error_json_details()),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let resolved_root = effigy::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_parse_error(&mut renderer, &resolved_root, &err.to_string());
            std::process::exit(2);
        }
    };
    let cmd = apply_global_json_flag(parsed, global_json_mode);
    let suppress_header = command_requests_json(&cmd, global_json_mode);
    let emit_json_envelope = suppress_header;
    let (command_kind, command_name) = command_kind_and_name(&cmd);
    let command_root = effigy::runner::resolve_command_root(&cmd);

    match cmd {
        Command::Help(topic) => {
            if suppress_header {
                let topic_label = help_topic_label(topic);
                let mut help_renderer = PlainRenderer::new(Vec::<u8>::new(), false);
                let _ = render_help(&mut help_renderer, topic);
                let rendered = String::from_utf8(help_renderer.into_inner()).unwrap_or_default();
                let payload = json!({
                    "schema": "effigy.help.v1",
                    "schema_version": 1,
                    "ok": true,
                    "topic": topic_label,
                    "text": rendered,
                });
                if emit_json_envelope {
                    emit_json_envelope_success_value(command_kind, &command_name, payload);
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
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            let _ = render_help(&mut renderer, topic);
            let _ = renderer.text("");
        }
        command @ (Command::Doctor(_) | Command::Tasks(_) | Command::Task(_)) => {
            run_and_render_command(
                output_mode,
                &command_root,
                suppress_header,
                emit_json_envelope,
                command_kind,
                &command_name,
                command,
            );
        }
    }
}
