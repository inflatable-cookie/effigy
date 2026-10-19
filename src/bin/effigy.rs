fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    // The per-run sentinel and supervisor re-exec this binary (contract 052
    // §5.8) before any CLI parsing, signal handling or spawn.
    if let Some(entry) = raw_args.first() {
        if let Some(code) = effigy::runner::run_internal_kernel_entry(entry) {
            std::process::exit(code);
        }
    }
    effigy::run_cli(raw_args);
    // run_cli exits internally on parse or runner error;
    // reaching here means success
}
