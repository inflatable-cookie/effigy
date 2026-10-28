//! Disposable Linux namespace proof; invoked only through the Effigy selector.
//!
//! The production adapter owns the capability, the launch unit and the probe;
//! this example only drives them from a throwaway container/VM so the
//! qualification route cannot drift from the code an operator runs.
#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    use effigy_gateway::linux_operator;
    let operator = nix::unistd::Uid::effective().as_raw();
    match std::env::args().nth(1).as_deref() {
        Some("install") => {
            linux_operator::install_current_capability(operator)?;
            println!("low-port capability installed on the running binary");
            Ok(())
        }
        Some("remove") => {
            linux_operator::remove_current_capability(operator)?;
            println!("low-port capability removed from the running binary");
            Ok(())
        }
        Some("probe") => {
            assert!(!nix::unistd::Uid::effective().is_root());
            for address in [
                "127.0.0.1:80",
                "127.0.0.1:443",
                "127.1.0.1:80",
                "127.1.0.50:443",
            ] {
                let listener = std::net::TcpListener::bind(address)?;
                assert_eq!(listener.local_addr()?.to_string(), address);
            }
            println!("unprivileged Linux loopback low-port binds passed");
            Ok(())
        }
        Some("report") => {
            println!(
                "{}",
                serde_json::to_string(&linux_operator::current_capability_state(operator))
                    .map_err(std::io::Error::other)?
            );
            Ok(())
        }
        Some("unit") => {
            // The unit is a pure function; the container has no user manager.
            let home = std::env::var_os("HOME")
                .map(std::path::PathBuf::from)
                .ok_or_else(|| std::io::Error::other("HOME is missing"))?;
            print!(
                "{}",
                String::from_utf8_lossy(&linux_operator::unit_definition(
                    &std::env::current_exe()?,
                    &home
                )?)
            );
            Ok(())
        }
        _ => Err(std::io::Error::other(
            "expected install, remove, probe, report or unit",
        )),
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {}
