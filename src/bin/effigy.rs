fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(error) = effigy::run_main(raw_args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
