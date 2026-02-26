use effigy::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use effigy::{parse_command, render_help, Command};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let output_mode = OutputMode::from_env();
    let cmd = match parse_command(args) {
        Ok(cmd) => cmd,
        Err(err) => {
            let mut renderer = PlainRenderer::stderr(output_mode);
            let _ = renderer.error_block(
                &MessageBlock::new("Invalid command arguments", err.to_string())
                    .with_hint("Run `effigy --help` to see supported command forms"),
            );
            let _ = render_help(&mut renderer);
            std::process::exit(2);
        }
    };

    match cmd {
        Command::Help => {
            let mut renderer = PlainRenderer::stdout(output_mode);
            let _ = render_help(&mut renderer);
        }
        _ => match effigy::runner::run_command(cmd) {
            Ok(output) => {
                if !output.trim().is_empty() {
                    let mut renderer = PlainRenderer::stdout(output_mode);
                    let _ = renderer.text(&output);
                }
            }
            Err(err) => {
                let mut renderer = PlainRenderer::stderr(output_mode);
                let _ = renderer.error_block(&MessageBlock::new("Task failed", err.to_string()));
                std::process::exit(1);
            }
        },
    }
}
