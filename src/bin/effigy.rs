fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    effigy::run_cli(raw_args);
}
