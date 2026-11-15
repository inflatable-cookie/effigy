fn main() {
    // The absolute run deadline is sampled once, before any capture, staging or
    // admission (effigy#992). A too-short deadline refuses here, before
    // anything starts.
    if let Err(error) = effigy_core::run_deadline::configure_from_env() {
        eprintln!("effigy: {error}");
        std::process::exit(2);
    }
    if let Err(error) = effigy_core::output_cap::configure_from_env() {
        eprintln!("effigy: {error}");
        std::process::exit(2);
    }
    let raw_args: Vec<String> = std::env::args().skip(1).collect();
    if raw_args == ["__worktree-archive-scratch-root"] {
        match effigy_manifest::load_user_config() {
            Ok(config) => match config.scratch.root() {
                Some(root) if root.is_absolute() && !root.to_string_lossy().contains('\n') => {
                    println!("{}", root.display());
                }
                Some(_) => {
                    eprintln!("archive scratch root must be an absolute single-line path");
                    std::process::exit(1);
                }
                None => std::process::exit(2),
            },
            Err(error) => {
                eprintln!("cannot load archive scratch configuration: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    #[cfg(feature = "workspace-private-qualification")]
    if effigy_kernel::supervise::initialize_private_qualification_image().is_err() {
        eprintln!("workspace qualification creator image unavailable");
        std::process::exit(1);
    }
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
        let _ = effigy_core::output_cap::write(
            effigy_core::output_cap::Stream::Stderr,
            format!("{error}\n").as_bytes(),
        );
        let _ = effigy_core::output_cap::finish();
        std::process::exit(1);
    }
    let _ = effigy_core::output_cap::finish();
}
