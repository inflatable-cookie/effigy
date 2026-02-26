use effigy::ui::{MessageBlock, OutputMode, PlainRenderer, Renderer};
use effigy::{parse_command, print_usage, Command};

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
            print_usage();
            std::process::exit(2);
        }
    };

    match cmd {
        Command::Help => {
            print_usage();
        }
        _ => match effigy::runner::run_command(cmd) {
            Ok(output) => {
                if !output.trim().is_empty() {
                    println!("{output}");
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
