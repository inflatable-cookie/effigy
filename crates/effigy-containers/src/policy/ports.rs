//! Pinned host ports.
//!
//! Host ports come from Compose or the OS unless the manifest pins one, so
//! there is nothing to allocate or release here.
use std::collections::BTreeSet;

use effigy_gateway::ports::{port_holder, port_is_free};

/// The concrete host port a declared binding pins, if it pins one.
///
/// A binding is `[<publish-address>:]<host>:<container>`. Compose's
/// `127.0.0.1::<container>` form leaves the host empty: the OS assigns it and
/// nothing pins anything.
pub fn pinned_host_port(declared: &str) -> Option<u16> {
    let mut parts = declared.rsplit(':');
    let container = parts.next()?.trim();
    let host = parts.next().unwrap_or_default().trim();
    if container.is_empty() || host.is_empty() {
        return None;
    }
    host.parse().ok()
}

/// Refuse a manifest pin whose port another process already holds, naming the
/// holder. A port this project's own containers publish is not a conflict:
/// Compose reuses its own binding.
pub fn pinned_host_port_conflict(
    container: &str,
    declared_ports: &[String],
    owned_ports: &BTreeSet<u16>,
) -> Option<String> {
    for declared in declared_ports {
        let Some(port) = pinned_host_port(declared) else {
            continue;
        };
        if owned_ports.contains(&port) || port_is_free(port) {
            continue;
        }
        let holder = port_holder(port).unwrap_or_else(|| "another process".to_owned());
        return Some(format!(
            "container `{container}` pins host port {port} ({declared}), but 127.0.0.1:{port} is already held by {holder}; stop that process or change `[containers.{container}.host].ports`"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_os_assigned_binding_pins_nothing() {
        assert_eq!(pinned_host_port("127.0.0.1::80"), None);
        assert_eq!(pinned_host_port("::80"), None);
        assert_eq!(pinned_host_port("18080:80"), Some(18080));
        assert_eq!(pinned_host_port("127.0.0.1:18080:80"), Some(18080));
    }

    #[test]
    fn an_os_assigned_port_is_never_a_collision() {
        assert_eq!(
            pinned_host_port_conflict("web", &["127.0.0.1::80".to_owned()], &BTreeSet::new()),
            None
        );
    }

    #[test]
    fn a_pinned_port_that_another_process_holds_names_the_holder() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve a port held by this test");
        let port = listener.local_addr().expect("bound address").port();

        let message =
            pinned_host_port_conflict("web", &[format!("127.0.0.1:{port}:80")], &BTreeSet::new())
                .expect("a held pin is a conflict");

        assert!(message.contains(&port.to_string()), "{message}");
        assert!(message.contains("held by"), "{message}");
        drop(listener);
    }

    #[test]
    fn a_pinned_port_this_project_already_publishes_is_not_a_conflict() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .expect("reserve a port held by this test");
        let port = listener.local_addr().expect("bound address").port();
        let owned = BTreeSet::from([port]);

        assert_eq!(
            pinned_host_port_conflict("web", &[format!("127.0.0.1:{port}:80")], &owned),
            None
        );
        drop(listener);
    }
}
