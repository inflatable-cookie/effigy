fn main() {
    // Capture and remove the internal presentation marker before any worker
    // threads or subprocesses start. Descendants must not inherit it.
    // SAFETY: this is the first operation in main, before this binary starts
    // any threads or invokes code that can access the process environment.
    let ansi_front = unsafe { effigy_kernel::startup::take_variable("EFFIGY_INTERNAL_ANSI_FRONT") };
    effigy::runner::set_startup_ansi_front(ansi_front);
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(target_os = "macos")]
    if raw_args
        .first()
        .is_some_and(|arg| arg == "__gateway-bind-broker")
    {
        let result = match raw_args.as_slice() {
            [_, uid] => uid
                .parse()
                .map_err(std::io::Error::other)
                .and_then(effigy_gateway::bind_broker::entry),
            _ => Err(std::io::Error::other("expected setup-pinned operator UID")),
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    if raw_args
        .first()
        .is_some_and(|arg| arg == effigy_gateway::linux_operator::BIND_PROBE_ENTRY)
    {
        // A hidden setup child: it must be a fresh exec of the installed
        // binary so the file capability is effective, and it binds nothing
        // after its bounded probe.
        let addresses = raw_args[1..]
            .iter()
            .map(|value| value.parse())
            .collect::<Result<Vec<std::net::SocketAddr>, _>>();
        let result = match addresses {
            Ok(addresses) if !addresses.is_empty() => {
                effigy_gateway::linux_operator::probe_entry(&addresses)
            }
            _ => Err(std::io::Error::other(
                "expected one or more loopback socket addresses",
            )),
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
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
