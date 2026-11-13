//! Test-process guard for the operator gateway's machine-owned endpoints.

use std::io;
use std::net::SocketAddr;
use std::path::PathBuf;

use crate::server::GatewayConfig;

/// Library-driven fixtures must select a private state directory and
/// unprivileged loopback ports. CLI fixtures use `GatewayConfig::private`.
pub(super) fn require_fixture_isolation(config: &GatewayConfig) -> io::Result<()> {
    if !cfg!(debug_assertions)
        || cfg!(test)
        || !std::env::vars_os()
            .any(|(key, _)| key.to_string_lossy().starts_with("EFFIGY_TEST_") || key == "NEXTEST")
    {
        return Ok(());
    }
    let selected =
        std::env::var_os(crate::private_state::PRIVATE_STATE_ROOT_ENV).map(PathBuf::from);
    if let Some(root) =
        selected.filter(|root| Some(root.as_path()) == config.pid_file_path.parent())
    {
        if test_endpoints_are_unprivileged(config) {
            return crate::private_state::validate_root(&root).map_err(super::error);
        }
    }
    Err(super::error("fixture process requires a private gateway root and test-only ports; refusing operator gateway mutation"))
}

fn test_endpoints_are_unprivileged(config: &GatewayConfig) -> bool {
    [
        Some(config.dns.bind_addr),
        Some(config.proxy.bind_addr),
        config.proxy.tls_bind_addr,
    ]
    .into_iter()
    .flatten()
    .all(|addr: SocketAddr| addr.ip().is_loopback() && addr.port() >= 1024 && addr.port() != 15353)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_endpoint_guard_requires_loopback_test_ports() {
        let root = tempfile::tempdir().unwrap();
        let mut config = GatewayConfig::standard(root.path().join(".effigy/gateway"));
        config.dns.bind_addr = "127.0.0.1:15353".parse().unwrap();
        config.proxy.bind_addr = "127.0.0.1:18080".parse().unwrap();
        config.proxy.tls_bind_addr = None;
        assert!(!test_endpoints_are_unprivileged(&config));

        config.dns.bind_addr = "127.0.0.1:15354".parse().unwrap();
        assert!(test_endpoints_are_unprivileged(&config));
        config.proxy.bind_addr = "0.0.0.0:18080".parse().unwrap();
        assert!(!test_endpoints_are_unprivileged(&config));
    }
}
