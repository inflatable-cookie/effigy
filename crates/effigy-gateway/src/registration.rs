//! Route registration helpers for container lifecycle events.
//!
//! When a container starts, it needs to register its domain with the
//! gateway route table. When it stops, the route needs to be removed.
//! This module provides the logic for those operations:
//!
//! - Building route entries from container configuration
//! - Registration and deregistration in one endpoint transaction
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
use crate::routes::{ManagedListenerRouteOwner, Route, RouteSource};

/// How long a recorded target has to accept a TCP connection before another
/// checkout may treat it as gone and take its domain over.
const TARGET_PROBE_TIMEOUT: Duration = Duration::from_millis(200);

/// Whether a route belongs to this checkout path.
pub fn owned_by(route: &Route, project_path: &str) -> bool {
    route.project == project_path
}

/// Whether the checkout that recorded a route is gone from disk.
///
/// Only a missing path is gone. An unreadable path (EACCES, a stale network
/// handle) is unknown, and so is a path whose own parent is missing: an
/// unmounted `/Volumes/...` root looks exactly like a deleted checkout, and
/// dropping its routes would leave a running stack unroutable after its
/// remount. Keeping a route of a checkout that really is gone only delays its
/// release; an ordinary event that deletes a worktree still frees its domains,
/// because the parent of a deleted worktree stays.
///
/// The route table is the whole record, so a route of a deleted worktree is
/// dropped by the next write rather than kept and guarded.
pub fn checkout_is_gone(project: &str) -> bool {
    let path = Path::new(project);
    match std::fs::metadata(path) {
        Ok(_) => false,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            path.parent().is_some_and(Path::is_dir)
        }
        Err(_) => false,
    }
}

/// Why another checkout may take a recorded owner's domain over: its checkout
/// path does not exist, or its target address does not accept a connection.
/// This is the tradeover decision only. Nothing prunes a route because its
/// target is briefly down, so a container restart, a Colima restart or a
/// listening process that has not bound yet keeps its route. `None` means the
/// owner is still there and keeps the domain.
pub fn dead_claim_reason(route: &Route) -> Option<String> {
    if checkout_is_gone(&route.project) {
        return Some(format!("checkout path {} no longer exists", route.project));
    }
    let target = route.target.as_deref().or(route.tcp_target.as_deref())?;
    if target_accepts_connection(target) {
        return None;
    }
    Some(format!("target {target} is not accepting connections"))
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
    let parsed_target =
        target
            .parse::<SocketAddr>()
            .map_err(|error| GatewayError::RouteTableReadError {
                path: route_table_path.to_path_buf(),
                reason: format!("managed listener target is not a socket address: {error}"),
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
        table.upsert(Route {
            domain: domain.to_owned(),
            target: Some(target.to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Task,
            project: project_path.to_owned(),
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

/// Withdraw every route one checkout path owns.
///
/// The route table is the whole record of the checkout's claims, so this
/// withdraws the route of a domain the manifest has since renamed, a listener
/// route an earlier generation published, and any route this checkout's
/// environment left behind. A route another checkout owns stays, and a domain
/// that has no route is simply not in the result: withdrawing a route that is
/// already gone is not an error.
///
/// Certificates are a cache keyed by domain and outlive their route, so
/// nothing here removes one; the gateway daemon prunes the ones no route
/// names once they are old enough to be worth nothing.
pub fn withdraw_owned_routes(
    route_table_path: &Path,
    project_path: &str,
) -> Result<Vec<Route>, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let removed = table
            .all_routes()
            .into_iter()
            .filter(|route| owned_by(route, project_path))
            .cloned()
            .collect::<Vec<_>>();
        for route in &removed {
            let _ = table.deregister(&route.domain);
        }
        Ok(removed)
    })
}

#[cfg(test)]
#[path = "registration/tests.rs"]
mod tests;
