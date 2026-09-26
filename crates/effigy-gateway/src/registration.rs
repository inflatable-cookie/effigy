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

use crate::error::GatewayError;
use crate::ports::PortRegistry;
use crate::routes::RouteTableLock;
use crate::routes::{Route, RouteSource, RouteTable};
use effigy_core::worktree_scope;

/// Resolve the generation that owns container gateway routes.
pub fn project_scope(project_path: &str) -> Result<Option<String>, GatewayError> {
    worktree_scope::load_or_create(Path::new(project_path)).map_err(|error| {
        GatewayError::RouteTableReadError {
            path: Path::new(project_path).to_path_buf(),
            reason: format!("cannot establish worktree route owner: {error}"),
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
    let live = match route.scope.as_deref() {
        Some(token) => worktree_scope::is_live(Path::new(&route.project), token),
        None => Path::new(&route.project).exists(),
    };
    if live || route.source != RouteSource::Container {
        return Err(GatewayError::ForeignRoute {
            domain: route.domain.clone(),
            project: route.project.clone(),
        });
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

/// Register a route in the route table file.
///
/// Loads the route table, adds the route, and saves atomically.
/// If a route for the domain already exists, it's replaced (upsert).
pub fn register_route(
    route_table_path: &Path,
    registration: &RouteRegistration,
) -> Result<(), GatewayError> {
    let _lock = RouteTableLock::acquire(route_table_path)?;
    let mut table = RouteTable::load(route_table_path)?;
    let scope = project_scope(&registration.project_path)?;
    if let Some(existing) = table.lookup(&registration.domain) {
        check_claim(existing, &registration.project_path, scope.as_deref())?;
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
        registered: Utc::now(),
    });

    table.save(route_table_path)?;
    Ok(())
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
    let _lock = RouteTableLock::acquire(route_table_path)?;
    let mut table = RouteTable::load(route_table_path)?;
    let Ok(scope) = project_scope(project_path) else {
        return Ok(false);
    };
    let owned = table
        .lookup(domain)
        .is_some_and(|route| owned_by(route, project_path, scope.as_deref()));
    if !owned {
        return Ok(false);
    }
    let _ = table.deregister(domain);
    table.save(route_table_path)?;
    Ok(true)
}

/// Deregister all routes for a project path.
///
/// Useful when tearing down a container that may have registered
/// multiple domains (e.g., main domain + aliases).
pub fn deregister_project_routes(
    route_table_path: &Path,
    project_path: &str,
) -> Result<usize, GatewayError> {
    let _lock = RouteTableLock::acquire(route_table_path)?;
    let mut table = RouteTable::load(route_table_path)?;
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

    if count > 0 {
        table.save(route_table_path)?;
    }

    Ok(count)
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
