fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    effigy::run_cli(raw_args);
    // run_cli exits internally on parse or runner error;
    // local rehearsal rebuild marker
    // reaching here means success
}
