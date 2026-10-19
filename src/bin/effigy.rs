fn main() {
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args == ["__gateway-run"]
        || raw_args
            .first()
            .is_some_and(|arg| arg == "__gateway-user-run")
    {
        let addresses = match raw_args.as_slice() {
            [_] => None,
            [_, dns, http, https] => match (dns.parse(), http.parse(), https.parse()) {
                (Ok(dns), Ok(http), Ok(https)) => Some([dns, http, https]),
                _ => {
                    eprintln!("operator endpoints must be socket addresses");
                    std::process::exit(2);
                }
            },
            _ => {
                eprintln!("expected DNS, HTTP and HTTPS operator endpoints");
                std::process::exit(2);
            }
        };
        if let Err(error) = effigy_gateway::operator::daemon_entry(addresses) {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = effigy::run_main(raw_args) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
