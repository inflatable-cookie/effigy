//! Route registration helpers for container lifecycle events.
//!
//! When a container starts, it needs to register its domain with the
//! gateway route table. When it stops, the route needs to be removed.
//! This module provides the logic for those operations, including:
//!
//! - Building route entries from container configuration
//! - Atomic registration/deregistration with the route table file
//! - Port mapping integration (using allocated ports when available)
//!
//! The actual container lifecycle detection is handled by the caller
//! (the container command in the runner). This module provides the
//! pure registration logic.

use std::net::Ipv4Addr;
use std::path::Path;

use chrono::Utc;

use crate::endpoint_store::EndpointStore;
use crate::error::GatewayError;
use crate::ports::PortRegistry;
#[cfg(test)]
use crate::routes::RouteTable;
use crate::routes::{ManagedListenerRouteOwner, Route, RouteSource};
use effigy_core::worktree_scope;

/// Resolve the generation that owns container gateway routes.
pub fn project_scope(project_path: &str) -> Result<Option<String>, GatewayError> {
    worktree_scope::load_or_create(Path::new(project_path)).map_err(|error| {
        GatewayError::RouteTableReadError {
            path: Path::new(project_path).to_path_buf(),
            reason: format!("cannot establish route owner generation: {error}"),
        }
    })
}

/// Whether a route still belongs to this exact checkout generation.
pub fn owned_by(route: &Route, project_path: &str, scope: Option<&str>) -> bool {
    route.project == project_path && route.scope.as_deref() == scope
}

/// Refuse a live foreign owner; an absent worktree generation is stale.
pub fn check_claim(
    route: &Route,
    project_path: &str,
    scope: Option<&str>,
) -> Result<(), GatewayError> {
    if owned_by(route, project_path, scope) {
        return Ok(());
    }
    let live = match std::fs::metadata(&route.project) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => true,
        Ok(_) => match route.scope.as_deref() {
            Some(token) => match worktree_scope::load_scoped(Path::new(&route.project)) {
                Ok(Some((_, current))) => current == token,
                Ok(None) => false,
                Err(_) => !recorded_git_directory_absent(Path::new(&route.project)),
            },
            None => true,
        },
    };
    if live || route.source != RouteSource::Container {
        return Err(GatewayError::ForeignRoute {
            domain: route.domain.clone(),
            project: route.project.clone(),
        });
    }
    Ok(())
}

fn recorded_git_directory_absent(project: &Path) -> bool {
    let Ok(Some(bytes)) = crate::endpoint_store::import::read_input(&project.join(".git")) else {
        return false;
    };
    if bytes.len() > 4096 {
        return false;
    }
    let Ok(pointer) = std::str::from_utf8(&bytes) else {
        return false;
    };
    let Some(pointer) = pointer
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|p| !p.is_empty())
    else {
        return false;
    };
    let pointer = Path::new(pointer);
    let path = if pointer.is_absolute() {
        pointer.to_owned()
    } else {
        project.join(pointer)
    };
    matches!(std::fs::symlink_metadata(path), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
}

/// Whether container teardown still covers a route for this checkout
/// generation. Teardown removes only routes this checkout published as
/// container routes; a managed listener route, including one handed over from
/// this checkout's own container environment, stays.
pub fn container_teardown_covers(route: &Route, project_path: &str, scope: Option<&str>) -> bool {
    owned_by(route, project_path, scope)
        && route.managed_listener.is_none()
        && route.source == RouteSource::Container
}

/// Refuse a container registration that would displace a live managed listener
/// from this same checkout and scope. A managed listener generation that is no
/// longer live is a handover, not a conflict.
pub fn check_container_claim(
    route: &Route,
    project_path: &str,
    scope: Option<&str>,
) -> Result<(), GatewayError> {
    check_container_claim_with(
        route,
        project_path,
        scope,
        crate::managed_listener::observe_managed_listener_owner,
    )
}

/// [`check_container_claim`] with an injected liveness observation, so tests
/// can exercise both outcomes without a real listener process.
pub fn check_container_claim_with<F>(
    route: &Route,
    project_path: &str,
    scope: Option<&str>,
    listener_is_live: F,
) -> Result<(), GatewayError>
where
    F: FnOnce(&ManagedListenerRouteOwner) -> effigy_kernel::observe::Observed<bool>,
{
    check_claim(route, project_path, scope)?;
    if !owned_by(route, project_path, scope) {
        return Ok(());
    }
    if let Some(owner) = route.managed_listener.as_ref() {
        if !matches!(
            listener_is_live(owner),
            effigy_kernel::observe::Observed::Verified(false, _)
                | effigy_kernel::observe::Observed::Absent(_)
        ) {
            return Err(GatewayError::ManagedListenerRouteConflict {
                domain: route.domain.clone(),
                owner: owner.owner.clone(),
                pid: owner.supervisor_pid,
            });
        }
    }
    Ok(())
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

/// How a managed listener route was published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManagedListenerRoutePublication {
    /// No route previously held the domain.
    Registered,
    /// Replaced the same managed owner's previous child generation.
    ReplacedGeneration,
    /// Took over a route published by this checkout's container environment.
    HandedOverFromContainer,
}

/// Register a route in the route table file.
///
/// Loads the route table, adds the route, and saves atomically.
/// If a route for the domain already exists, it's replaced (upsert), except a
/// live managed listener from this same checkout and scope, which stays.
pub fn register_route(
    route_table_path: &Path,
    registration: &RouteRegistration,
) -> Result<(), GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let scope = project_scope(&registration.project_path)?;
        if let Some(existing) = table.lookup(&registration.domain) {
            check_container_claim(existing, &registration.project_path, scope.as_deref())?;
        }

        table.upsert(Route {
            domain: registration.domain.clone(),
            target: registration.target.clone(),
            dns_ip: registration.dns_ip,
            tcp_port: registration.tcp_port,
            tcp_target: registration.tcp_target.clone(),
            source: registration.source,
            project: registration.project_path.clone(),
            tls: registration.tls,
            scope,
            managed_listener: None,
            registered: Utc::now(),
        });

        Ok(())
    })
}

/// Atomically publish a host-listener route for one managed process
/// generation. A same-project route is not sufficient authority: replacement
/// of another managed generation requires the exact stable listener owner and
/// expected predecessor generation. A route from this same checkout's container
/// environment is handed over in the same locked transaction.
pub fn register_managed_listener_route(
    route_table_path: &Path,
    project_path: &str,
    domain: &str,
    target: &str,
    tls: bool,
    owner: ManagedListenerRouteOwner,
    expected_generation: Option<&str>,
) -> Result<ManagedListenerRoutePublication, GatewayError> {
    let parsed_target = target.parse::<std::net::SocketAddr>().map_err(|error| {
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
        let scope = project_scope(project_path)?;
        let publication = if let Some(existing) = table.lookup(domain) {
            if owned_by(existing, project_path, scope.as_deref()) {
                if let Some(current) = existing.managed_listener.as_ref() {
                    if current.owner == owner.owner
                        && expected_generation == Some(current.generation.as_str())
                    {
                        ManagedListenerRoutePublication::ReplacedGeneration
                    } else {
                        return Err(GatewayError::ForeignRoute {
                            domain: domain.to_owned(),
                            project: existing.project.clone(),
                        });
                    }
                } else if existing.source == RouteSource::Container && expected_generation.is_none()
                {
                    ManagedListenerRoutePublication::HandedOverFromContainer
                } else {
                    return Err(GatewayError::ForeignRoute {
                        domain: domain.to_owned(),
                        project: existing.project.clone(),
                    });
                }
            } else {
                return Err(GatewayError::ForeignRoute {
                    domain: domain.to_owned(),
                    project: existing.project.clone(),
                });
            }
        } else if expected_generation.is_some() {
            return Err(GatewayError::RouteTableReadError {
                path: route_table_path.to_path_buf(),
                reason: format!(
                    "managed listener route `{domain}` disappeared before generation replacement"
                ),
            });
        } else {
            ManagedListenerRoutePublication::Registered
        };

        table.upsert(Route {
            domain: domain.to_owned(),
            target: Some(target.to_owned()),
            dns_ip: None,
            tcp_port: None,
            tcp_target: None,
            source: RouteSource::Task,
            project: project_path.to_owned(),
            scope,
            tls,
            managed_listener: Some(owner),
            registered: Utc::now(),
        });
        Ok(publication)
    })
}

/// Check a requested listener domain before child startup. Existing claims are
/// accepted when they already belong to the exact stable managed owner, or when
/// they are container routes from this same checkout generation that the
/// publication transaction will hand over.
pub fn check_managed_listener_route_claim(
    route_table_path: &Path,
    project_path: &str,
    domain: &str,
    owner: &str,
) -> Result<(), GatewayError> {
    EndpointStore::for_routes(route_table_path).edit(None, |snapshot| {
        let table = snapshot.routes()?;
        let scope = project_scope(project_path)?;
        if let Some(existing) = table.lookup(domain) {
            let same_owner = existing
                .managed_listener
                .as_ref()
                .is_some_and(|current| current.owner == owner);
            let container_handover =
                existing.managed_listener.is_none() && existing.source == RouteSource::Container;
            if !owned_by(existing, project_path, scope.as_deref())
                || !(same_owner || container_handover)
            {
                return Err(GatewayError::ForeignRoute {
                    domain: domain.to_owned(),
                    project: existing.project.clone(),
                });
            }
        }
        snapshot.reserve_domain(domain, project_path, scope.as_deref(), Some(owner))
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
        let scope = project_scope(project_path)?;
        let matches = table.lookup(domain).is_some_and(|route| {
            owned_by(route, project_path, scope.as_deref())
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
        let Ok(scope) = project_scope(project_path) else {
            return Ok(false);
        };
        let owned = table
            .lookup(domain)
            .is_some_and(|route| container_teardown_covers(route, project_path, scope.as_deref()));
        if !owned {
            return Ok(false);
        }
        let _ = table.deregister(domain);
        Ok(true)
    })
}

/// Deregister all routes for a project path.
///
/// Useful when tearing down a container that may have registered
/// multiple domains (e.g., main domain + aliases).
pub fn deregister_project_routes(
    route_table_path: &Path,
    project_path: &str,
) -> Result<usize, GatewayError> {
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let Ok(scope) = project_scope(project_path) else {
            return Ok(0);
        };

        let domains_to_remove: Vec<String> = table
            .all_routes()
            .iter()
            .filter(|r| owned_by(r, project_path, scope.as_deref()))
            .map(|r| r.domain.clone())
            .collect();

        let count = domains_to_remove.len();
        for domain in &domains_to_remove {
            let _ = table.deregister(domain);
        }

        Ok(count)
    })
}

/// Remove routes owned by a recorded checkout generation.
///
/// Uses the durable token rather than the live Git worktree directory, so
/// cleanup can retry after the checkout is gone.
pub fn deregister_owned_routes(
    route_table_path: &Path,
    project_path: &str,
    scope: Option<&str>,
) -> Result<Vec<Route>, GatewayError> {
    deregister_owned_routes_with(route_table_path, project_path, scope, |_| Ok(()))
}

/// Remove owned routes and run `on_removed` before releasing the route lock.
///
/// TLS certificate deletion belongs here so another checkout cannot claim the
/// domain and then lose its certificate. The callback runs before the table is
/// saved; a callback error leaves the route in place.
pub fn deregister_owned_routes_with<F>(
    route_table_path: &Path,
    project_path: &str,
    scope: Option<&str>,
    on_removed: F,
) -> Result<Vec<Route>, GatewayError>
where
    F: FnMut(&Route) -> Result<(), GatewayError>,
{
    deregister_owned_routes_matching(route_table_path, project_path, scope, None, on_removed)
}

/// Remove owned routes whose domains are in `domains`, then save the table.
///
/// `domains = None` removes every route owned by the checkout generation.
/// `on_removed` runs while the route is still recorded; failure skips save.
pub fn deregister_owned_routes_matching<F>(
    route_table_path: &Path,
    project_path: &str,
    scope: Option<&str>,
    domains: Option<&[String]>,
    mut on_removed: F,
) -> Result<Vec<Route>, GatewayError>
where
    F: FnMut(&Route) -> Result<(), GatewayError>,
{
    EndpointStore::for_routes(route_table_path).routes(|table| {
        let owned = table
            .all_routes()
            .into_iter()
            .filter(|route| owned_by(route, project_path, scope))
            .filter(|route| {
                domains
                    .map(|allowed| allowed.iter().any(|domain| domain == &route.domain))
                    .unwrap_or(true)
            })
            .cloned()
            .collect::<Vec<_>>();
        if owned.is_empty() {
            return Ok(owned);
        }
        for route in &owned {
            on_removed(route)?;
            let _ = table.deregister(&route.domain);
        }
        Ok(owned)
    })
}

/// Remove owned routes and leftover TLS certificates under the route lock.
///
/// `remove_cert` runs only when the domain is still owned by this generation
/// or is unclaimed. A live foreign owner keeps its certificate and route.
/// Owned routes are deregistered after their certificates are removed; a
/// callback error leaves the table unchanged on disk.
pub fn cleanup_owned_routes_and_certs<F>(
    route_table_path: &Path,
    project_path: &str,
    scope: Option<&str>,
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
                    if owned_by(route, project_path, scope)
                        && !domains.iter().any(|existing| existing == &route.domain)
                    {
                        domains.push(route.domain.clone());
                    }
                }
            }
        }
        for domain in &domains {
            match table.lookup(domain).cloned() {
                Some(route) if owned_by(&route, project_path, scope) => {
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
