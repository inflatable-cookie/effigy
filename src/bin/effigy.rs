use effigy::ui::{OutputMode, PlainRenderer};
use effigy::{
    apply_global_json_flag, command_kind_and_name, command_requests_json, emit_json_envelope_error,
    parse_command, parse_error_json_details, render_parse_error, run_and_render_command,
    run_help_command, strip_global_json_flags, CliExecutionContext, Command,
};

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
    let context = CliExecutionContext {
        output_mode,
        command_root: &command_root,
        suppress_header,
        emit_json_envelope,
        command_kind,
        command_name: &command_name,
    };

    match cmd {
        Command::Help(topic) => {
            run_help_command(&context, topic);
        }
        command @ (Command::Doctor(_) | Command::Tasks(_) | Command::Task(_)) => {
            run_and_render_command(&context, command);
        }
    }
}
