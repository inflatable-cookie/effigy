use effigy::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use effigy::{
    apply_global_json_flag, command_requests_json, parse_command, render_cli_header, render_help,
    strip_global_json_flag, Command, HelpTopic,
};

fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    let (args, global_json_mode) = strip_global_json_flag(raw_args);
    let output_mode = OutputMode::from_env();
    let parsed = match parse_command(args) {
        Ok(cmd) => cmd,
        Err(err) => {
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
    let command_root = effigy::runner::resolve_command_root(&cmd);

    match cmd {
        Command::Help(topic) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            if !suppress_header {
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            let _ = render_help(&mut renderer, topic);
            let _ = renderer.text("");
        }
        Command::RepoPulse(args) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            if !suppress_header {
                let _ = render_cli_header(&mut renderer, &command_root);
            }
            match effigy::runner::run_command(Command::RepoPulse(args)) {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
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
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
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
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    if let Some(rendered) = err.rendered_output() {
                        let _ = renderer.text(rendered);
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
