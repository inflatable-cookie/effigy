//! Disposable Linux namespace proof; invoked only through the Effigy selector.
#[cfg(target_os = "linux")]
fn main() -> std::io::Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("install") => effigy_gateway::setup::install_bind_capability(),
        Some("remove") => effigy_gateway::setup::remove_bind_capability(),
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
        _ => Err(std::io::Error::other("expected install, remove or probe")),
    }
}
#[cfg(not(target_os = "linux"))]
fn main() {}
