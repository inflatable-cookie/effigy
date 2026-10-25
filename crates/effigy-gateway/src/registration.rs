//! Route registration helpers for container lifecycle events.
//!
//! When a container starts, it needs to register its domain with the
//! gateway route table. When it stops, the route needs to be removed.
//! This module provides the logic for those operations:
//!
//! - Building route entries from container configuration
//! - Atomic registration/deregistration with the route table file
//! - Port mapping integration (using allocated ports when available)
//!
//! A route is one claim: `domain → (checkout path, target)`. The checkout path
//! is the whole owner, so the same checkout always replaces its own route and
//! a different checkout takes the domain over only when the recorded owner is
//! dead. The actual container lifecycle detection is handled by the caller
//! (the container command in the runner).

use std::net::{Ipv4Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::time::Duration;

use chrono::Utc;

use crate::endpoint_store::EndpointStore;
use crate::error::GatewayError;
use crate::ports::PortRegistry;
use crate::routes::{ManagedListenerRouteOwner, Route, RouteSource};
use effigy_core::worktree_scope;

/// How long a recorded target has to accept a TCP connection before its owner
/// counts as gone.
const TARGET_PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// Resolve the generation that owns container gateway routes.
pub fn project_scope(project_path: &str) -> Result<Option<String>, GatewayError> {
    worktree_scope::load_or_create(Path::new(project_path)).map_err(|error| {
        GatewayError::RouteTableReadError {
            path: Path::new(project_path).to_path_buf(),
            reason: format!("cannot establish route owner generation: {error}"),
        }
    })
}

/// Whether a route belongs to this checkout path.
pub fn owned_by(route: &Route, project_path: &str) -> bool {
    route.project == project_path
}

/// Why a recorded owner is gone: its checkout path does not exist, or its
/// target address does not accept a connection. `None` means the owner is
/// still there and keeps the domain.
pub fn dead_claim_reason(route: &Route) -> Option<String> {
    if std::fs::metadata(&route.project).is_err() {
        return Some(format!("checkout path {} no longer exists", route.project));
    }
    let target = route.target.as_deref().or(route.tcp_target.as_deref())?;
    if target_accepts_connection(target) {
        return None;
    }
    Some(format!("target {target} is not accepting connections"))
}

/// Whether a recorded owner is gone. A route without an upstream target (a
/// DNS-only alias) is dead only when its checkout path is gone.
pub fn claim_is_dead(route: &Route) -> bool {
    dead_claim_reason(route).is_some()
}

fn target_accepts_connection(target: &str) -> bool {
    let addresses = match target.parse::<SocketAddr>() {
        Ok(address) => vec![address],
        Err(_) => match target.to_socket_addrs() {
            Ok(addresses) => addresses.collect::<Vec<_>>(),
            Err(_) => return false,
        },
    };
    addresses.into_iter().any(|address| {
        TcpStream::connect_timeout(&address, TARGET_PROBE_TIMEOUT)
            .map(|stream| {
                let _ = stream.shutdown(std::net::Shutdown::Both);
            })
            .is_ok()
    })
}

/// Register a route in the route table file.
///
/// Loads the route table, adds the route, and saves atomically. The same
/// checkout path replaces its own route; another checkout's live claim refuses
/// the write inside the same locked transaction.
pub fn register_route(
    route_table_path: &Path,
    registration: &RouteRegistration,
) -> Result<(), GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        table.upsert(Route {
            domain: registration.domain.clone(),
            target: registration.target.clone(),
            dns_ip: registration.dns_ip,
            tcp_port: registration.tcp_port,
            tcp_target: registration.tcp_target.clone(),
            source: registration.source,
            project: registration.project_path.clone(),
            tls: registration.tls,
            scope: project_scope(&registration.project_path)?,
            managed_listener: None,
            registered: Utc::now(),
        });

        Ok(())
    })
}

/// Configuration for registering a container route.
#[derive(Debug, Clone)]
pub struct RouteRegistration {
    /// The domain to register (e.g., "myproject.test").
    pub domain: String,

    /// Optional upstream host and port (e.g., "127.0.0.1:8080").
    pub target: Option<String>,

    /// Optional DNS IP override for this route.
    pub dns_ip: Option<Ipv4Addr>,

    /// Optional TCP bind port for a DNS-only service alias listener.
    pub tcp_port: Option<u16>,

    /// Optional upstream target for a DNS-only service alias listener.
    pub tcp_target: Option<String>,

    /// Whether TLS is enabled for this route.
    pub tls: bool,

    /// Absolute path to the project directory.
    pub project_path: String,

    /// How this route was registered.
    pub source: RouteSource,
}

/// Publish a host-listener route for one managed process generation. The same
/// checkout path replaces its own route, including a previous generation's or
/// its own container environment's route. Another checkout's live claim is
/// refused inside the same locked transaction.
pub fn register_managed_listener_route(
    route_table_path: &Path,
    project_path: &str,
    domain: &str,
    target: &str,
    tls: bool,
    owner: ManagedListenerRouteOwner,
) -> Result<(), GatewayError> {
    let parsed_target = target.parse::<SocketAddr>().map_err(|error| {
        GatewayError::RouteTableReadError {
            path: route_table_path.to_path_buf(),
            reason: format!("managed listener target is not a socket address: {error}"),
        }
    })?;
    if !parsed_target.ip().is_loopback()
        || parsed_target.port() == 0
        || owner.address != parsed_target.to_string()
        || owner.route_tls != tls
        || owner.owner.is_empty()
        || owner.runtime_generation.is_empty()
        || owner.generation.is_empty()
    {
        return Err(GatewayError::RouteTableReadError {
            path: route_table_path.to_path_buf(),
            reason: "managed listener route identity does not match its assigned loopback target"
                .to_owned(),
        });
    }
    EndpointStore::for_routes(route_table_path).routes(|table| {
        crate::managed_listener::verify_managed_listener_owner(&owner).map_err(|error| {
            GatewayError::RouteTableReadError {
                path: route_table_path.to_path_buf(),
                reason: format!("managed listener ownership refused publication: {error}"),
            }
        })?;
        table.upsert(Route {
            domain: domain.to_owned(),
            target: Some(target.to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Task,
            project: project_path.to_owned(),
            scope: project_scope(project_path)?,
            tls,
            managed_listener: Some(owner),
            registered: Utc::now(),
        });
        Ok(())
    })
}

/// Remove a managed listener route only when its stable owner and exact child
/// generation still match. A stale stop or exit callback cannot remove a
/// successor route.
pub fn deregister_managed_listener_route(
    route_table_path: &Path,
    project_path: &str,
    domain: &str,
    owner: &str,
    generation: &str,
) -> Result<bool, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let matches = table.lookup(domain).is_some_and(|route| {
            owned_by(route, project_path)
                && route.managed_listener.as_ref().is_some_and(|current| {
                    current.owner == owner && current.generation == generation
                })
        });
        if !matches {
            return Ok(false);
        }
        let _ = table.deregister(domain);
        Ok(true)
    })
}

/// Remove every managed listener route one stable listener owner published for
/// a domain, whatever child generation or run published it.
///
/// The supervisor that published a route is not consulted: a route left by a
/// run that died, was refused or was already closed is still the environment's
/// route, and the environment's own stop must withdraw it. Only this checkout's
/// routes for the given listener owner and domains are removed; a foreign
/// route, or another environment's listener, stays.
pub fn deregister_managed_listener_routes_for_owner(
    route_table_path: &Path,
    project_path: &str,
    owner: &str,
    domains: &[String],
) -> Result<Vec<Route>, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let owned = table
            .all_routes()
            .into_iter()
            .filter(|route| owned_by(route, project_path))
            .filter(|route| {
                domains
                    .iter()
                    .any(|domain| domain.eq_ignore_ascii_case(&route.domain))
            })
            .filter(|route| {
                route
                    .managed_listener
                    .as_ref()
                    .is_some_and(|current| current.owner == owner)
            })
            .cloned()
            .collect::<Vec<_>>();
        for route in &owned {
            let _ = table.deregister(&route.domain);
        }
        Ok(owned)
    })
}

/// Deregister a route from the route table file.
///
/// Loads the route table, removes the route, and saves atomically.
/// Returns Ok even if the route wasn't found (idempotent teardown).
pub fn deregister_route(
    route_table_path: &Path,
    domain: &str,
    project_path: &str,
) -> Result<bool, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let owned = table
            .lookup(domain)
            .is_some_and(|route| owned_by(route, project_path));
        if !owned {
            return Ok(false);
        }
        let _ = table.deregister(domain);
        Ok(true)
    })
}

/// Remove owned routes and leftover TLS certificates under the route lock.
///
/// `remove_cert` runs only when the domain is still owned by this checkout or
/// is unclaimed. A live foreign owner keeps its certificate and route. Owned
/// routes are deregistered after their certificates are removed; a callback
/// error leaves the table unchanged on disk.
pub fn cleanup_owned_routes_and_certs<F>(
    route_table_path: &Path,
    project_path: &str,
    route_domains: Option<&[String]>,
    cert_domains: &[String],
    mut remove_cert: F,
) -> Result<Vec<Route>, GatewayError>
where
    F: FnMut(&str) -> Result<(), GatewayError>,
{
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let mut removed = Vec::new();
        let mut domains = cert_domains.to_vec();
        match route_domains {
            Some(allowed) => {
                for domain in allowed {
                    if !domains.iter().any(|existing| existing == domain) {
                        domains.push(domain.clone());
                    }
                }
            }
            None => {
                for route in table.all_routes() {
                    if owned_by(route, project_path)
                        && !domains.iter().any(|existing| existing == &route.domain)
                    {
                        domains.push(route.domain.clone());
                    }
                }
            }
        }
        for domain in &domains {
            match table.lookup(domain).cloned() {
                Some(route) if owned_by(&route, project_path) => {
                    let remove_route = route_domains
                        .map(|allowed| allowed.iter().any(|allowed| allowed == domain))
                        .unwrap_or(true);
                    if route.tls || cert_domains.iter().any(|pending| pending == domain) {
                        remove_cert(domain)?;
                    }
                    if remove_route {
                        let _ = table.deregister(domain);
                        removed.push(route);
                    }
                }
                Some(_) => {}
                None => {
                    if cert_domains.iter().any(|pending| pending == domain) {
                        remove_cert(domain)?;
                    }
                }
            }
        }
        Ok(removed)
    })
}

/// Remove every route whose recorded owner is gone. The daemon runs this on a
/// timer so `gateway status` lists domains that something still serves.
pub fn prune_dead_routes(route_table_path: &Path) -> Result<Vec<String>, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let dead = table
            .all_routes()
            .into_iter()
            .filter(|route| claim_is_dead(route))
            .cloned()
            .collect::<Vec<_>>();
        for route in &dead {
            eprintln!(
                "[info] gateway domain `{}` released: {}",
                route.domain,
                dead_claim_reason(route).unwrap_or_else(|| "owner is gone".to_owned())
            );
            let _ = table.deregister(&route.domain);
        }
        Ok(dead.into_iter().map(|route| route.domain).collect())
    })
}

/// Build a route registration from container and port configuration.
///
/// If a port registry is available and the project has an allocation,
/// uses the allocated HTTP port. Otherwise, uses the provided default port.
pub fn build_registration(
    domain: &str,
    project_name: &str,
    project_path: &str,
    default_port: u16,
    tls: bool,
    port_registry: Option<&PortRegistry>,
) -> RouteRegistration {
    let port = port_registry
        .and_then(|reg| reg.port_map(project_name))
        .map(|pm| pm.http)
        .unwrap_or(default_port);

    RouteRegistration {
        domain: domain.to_string(),
        target: Some(format!("127.0.0.1:{port}")),
        dns_ip: None,
        tcp_port: None,
        tcp_target: None,
        tls,
        project_path: project_path.to_string(),
        source: RouteSource::Container,
    }
}

#[cfg(test)]
#[path = "registration/tests.rs"]
mod tests;
