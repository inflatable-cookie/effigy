use effigy::{parse_command, print_usage, Command};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = match parse_command(args) {
        Ok(cmd) => cmd,
        Err(err) => {
            eprintln!("{err}");
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
                eprintln!("task failed: {err}");
                std::process::exit(1);
            }
        },
    }
}
