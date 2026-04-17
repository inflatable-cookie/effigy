use crate::{
    emit_json_envelope_error, emit_json_envelope_success, parse_json_or_string, render_cli_header,
    CliExecutionContext, Command,
};
use effigy_core::widgets::MessageBlock;
use effigy_ui::{PlainRenderer, Renderer};

pub fn run_and_render_command(context: &CliExecutionContext<'_>, command: Command) {
    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !context.suppress_header {
        let _ = render_cli_header(&mut renderer, context.command_root);
    }
    match crate::runner::run_command(command) {
        Ok(output) => {
            if context.emit_json_envelope {
                emit_json_envelope_success(context.command_kind, context.command_name, &output);
                return;
            }
            if !output.trim().is_empty() {
                let _ = renderer.text(&output);
            }
            let _ = renderer.text("");
        }
        Err(err) => {
            if context.emit_json_envelope {
                emit_json_envelope_error(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    err.rendered_output().map(parse_json_or_string),
                );
            }
            if let Some(rendered) = err.rendered_output() {
                let _ = renderer.text(rendered);
                if context.suppress_header {
                    std::process::exit(1);
                }
            }
            if context.emit_json_envelope {
                emit_json_envelope_error(
                    1,
                    context.command_kind,
                    context.command_name,
                    "RunnerError",
                    &err.to_string(),
                    None,
                );
            }
            let mut err_renderer = PlainRenderer::stderr(context.output_mode);
            let _ = err_renderer.error_block(&MessageBlock::new("Task failed", err.to_string()));
            std::process::exit(1);
        }
    }
}
