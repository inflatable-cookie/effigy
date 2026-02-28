use effigy::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use effigy::{
    apply_global_json_flag, command_requests_json, parse_command, render_cli_header, render_help,
    strip_global_json_flags, Command, HelpTopic,
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
                    Some(json!({
                        "hint": "Run `effigy --help` to see supported command forms"
                    })),
                );
            }
            let mut renderer = PlainRenderer::stderr(output_mode);
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let resolved_root = effigy::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            if !global_json_mode {
                let _ = render_cli_header(&mut renderer, &resolved_root);
            }
            let _ = renderer.error_block(
                &MessageBlock::new("Invalid command arguments", err.to_string())
                    .with_hint("Run `effigy --help` to see supported command forms"),
            );
            let _ = render_help(&mut renderer, HelpTopic::General);
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
                let topic_label = match topic {
                    HelpTopic::General => "general",
                    HelpTopic::Doctor => "doctor",
                    HelpTopic::Tasks => "tasks",
                    HelpTopic::Test => "test",
                    HelpTopic::Watch => "watch",
                    HelpTopic::Init => "init",
                    HelpTopic::Migrate => "migrate",
                };
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
        Command::Doctor(args) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            if !suppress_header {
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            match effigy::runner::run_command(Command::Doctor(args)) {
                Ok(output) => {
                    if emit_json_envelope {
                        emit_json_envelope_success(command_kind, &command_name, &output);
                        return;
                    }
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if emit_json_envelope {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            err.rendered_output().map(parse_json_or_string),
                        );
                    }
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
                        if suppress_header {
                            std::process::exit(1);
                        }
                    }
                    if suppress_header {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            None,
                        );
                    }
                    let mut err_renderer = PlainRenderer::stderr(output_mode);
                    let _ = err_renderer
                        .error_block(&MessageBlock::new("Task failed", err.to_string()));
                    std::process::exit(1);
                }
            }
        }
        Command::Tasks(args) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            if !suppress_header {
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            match effigy::runner::run_command(Command::Tasks(args)) {
                Ok(output) => {
                    if emit_json_envelope {
                        emit_json_envelope_success(command_kind, &command_name, &output);
                        return;
                    }
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if emit_json_envelope {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            err.rendered_output().map(parse_json_or_string),
                        );
                    }
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
                        if suppress_header {
                            std::process::exit(1);
                        }
                    }
                    if suppress_header {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            None,
                        );
                    }
                    let mut err_renderer = PlainRenderer::stderr(output_mode);
                    let _ = err_renderer
                        .error_block(&MessageBlock::new("Task failed", err.to_string()));
                    std::process::exit(1);
                }
            }
        }
        Command::Task(task) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            if !suppress_header {
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            match effigy::runner::run_command(Command::Task(task)) {
                Ok(output) => {
                    if emit_json_envelope {
                        emit_json_envelope_success(command_kind, &command_name, &output);
                        return;
                    }
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if emit_json_envelope {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            err.rendered_output().map(parse_json_or_string),
                        );
                    }
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
                        if suppress_header {
                            std::process::exit(1);
                        }
                    }
                    if suppress_header {
                        emit_json_envelope_error(
                            1,
                            command_kind,
                            &command_name,
                            "RunnerError",
                            &err.to_string(),
                            None,
                        );
                    }
                    let mut err_renderer = PlainRenderer::stderr(output_mode);
                    let _ = err_renderer
                        .error_block(&MessageBlock::new("Task failed", err.to_string()));
                    std::process::exit(1);
                }
            }
        }
    }
}

fn command_kind_and_name(cmd: &Command) -> (&'static str, String) {
    match cmd {
        Command::Help(topic) => {
            let name = match topic {
                HelpTopic::General => "general",
                HelpTopic::Doctor => "doctor",
                HelpTopic::Tasks => "tasks",
                HelpTopic::Test => "test",
                HelpTopic::Watch => "watch",
                HelpTopic::Init => "init",
                HelpTopic::Migrate => "migrate",
            };
            ("help", name.to_owned())
        }
        Command::Doctor(_) => ("doctor", "doctor".to_owned()),
        Command::Tasks(_) => ("tasks", "tasks".to_owned()),
        Command::Task(task) => ("task", task.name.clone()),
    }
}

fn parse_json_or_string(raw: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(raw).unwrap_or_else(|_| json!({ "text": raw }))
}

fn emit_json_envelope_success(kind: &str, name: &str, output: &str) {
    let result = parse_json_or_string(output);
    emit_json_envelope_success_value(kind, name, result);
}

fn emit_json_envelope_success_value(kind: &str, name: &str, result: serde_json::Value) {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": true,
        "command": {
            "kind": kind,
            "name": name,
        },
        "result": result,
        "error": serde_json::Value::Null,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"kind\":\"JsonEncodeError\"}}".to_owned()
        })
    );
}

fn emit_json_envelope_error(
    exit_code: i32,
    kind: &str,
    name: &str,
    error_kind: &str,
    message: &str,
    details: Option<serde_json::Value>,
) -> ! {
    let payload = json!({
        "schema": "effigy.command.v1",
        "schema_version": 1,
        "ok": false,
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
    println!(
        "{}",
        serde_json::to_string_pretty(&payload).unwrap_or_else(|_| {
            "{\"ok\":false,\"error\":{\"kind\":\"JsonEncodeError\"}}".to_owned()
        })
    );
    std::process::exit(exit_code);
}
