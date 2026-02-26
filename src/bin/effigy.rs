use effigy::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use effigy::{parse_command, render_cli_header, render_help, Command, HelpTopic};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let output_mode = OutputMode::from_env();
    let cmd = match parse_command(args) {
        Ok(cmd) => cmd,
        Err(err) => {
            let mut renderer = PlainRenderer::stderr(output_mode);
            let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
            let resolved_root = effigy::resolver::resolve_target_root(cwd.clone(), None)
                .map_or(cwd, |r| r.resolved_root);
            let _ = render_cli_header(&mut renderer, &resolved_root);
            let _ = renderer.error_block(
                &MessageBlock::new("Invalid command arguments", err.to_string())
                    .with_hint("Run `effigy --help` to see supported command forms"),
            );
            let _ = render_help(&mut renderer, HelpTopic::General);
            std::process::exit(2);
        }
    };
    let command_root = effigy::runner::resolve_command_root(&cmd);

    match cmd {
        Command::Help(topic) => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            let _ = render_cli_header(&mut renderer, &command_root);
            let _ = render_help(&mut renderer, topic);
            let _ = renderer.text("");
        }
        _ => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            let _ = render_cli_header(&mut renderer, &command_root);
            match effigy::runner::run_command(cmd) {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        let _ = renderer.text(&output);
                    }
                    let _ = renderer.text("");
                }
                Err(err) => {
                    let mut err_renderer = PlainRenderer::stderr(output_mode);
                    let _ = err_renderer
                        .error_block(&MessageBlock::new("Task failed", err.to_string()));
                    std::process::exit(1);
                }
            }
        }
    }
}
