use effigy_test_support::process;
use std::path::Path;

fn main() {
    let args: Vec<_> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        Some("register") if args.len() == 4 => args[2]
            .parse::<u32>()
            .map_err(std::io::Error::other)
            .and_then(|pid| process::register_detached(pid, Path::new(&args[3])))
            .map(|_| false),
        Some("stop-root") if args.len() == 3 => {
            process::close_root(Path::new(&args[2])).map(|_| false)
        }
        Some("sweep") if args.len() == 3 => process::sweep(Path::new(&args[2]), None),
        _ => Err(std::io::Error::other(
            "usage: effigy-fixture-process register PID OWNER | sweep LEDGER",
        )),
    };
    match result {
        Ok(false) => (),
        Ok(true) => std::process::exit(1),
        Err(error) => {
            eprintln!("fixture process registry: {error}");
            std::process::exit(2);
        }
    }
}
