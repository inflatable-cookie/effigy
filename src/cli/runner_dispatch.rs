use std::path::Path;

use crate::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use crate::{
    emit_json_envelope_error, emit_json_envelope_success, parse_json_or_string, render_cli_header,
    Command,
};

pub fn run_and_render_command(
    output_mode: OutputMode,
    command_root: &Path,
    suppress_header: bool,
    emit_json_envelope: bool,
    command_kind: &str,
    command_name: &str,
    command: Command,
) {
    let mut renderer = PlainRenderer::stdout(output_mode);
    if !suppress_header {
        let _ = render_cli_header(&mut renderer, command_root);
    }
    match crate::runner::run_command(command) {
        Ok(output) => {
            if emit_json_envelope {
                emit_json_envelope_success(command_kind, command_name, &output);
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
                    command_name,
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
                    command_name,
                    "RunnerError",
                    &err.to_string(),
                    None,
                );
            }
            let mut err_renderer = PlainRenderer::stderr(output_mode);
            let _ = err_renderer.error_block(&MessageBlock::new("Task failed", err.to_string()));
            std::process::exit(1);
        }
    }
}
