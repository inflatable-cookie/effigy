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

use std::path::Path;

use chrono::Utc;

use crate::error::GatewayError;
use crate::ports::PortRegistry;
use crate::routes::{Route, RouteSource, RouteTable};

/// Configuration for registering a container route.
#[derive(Debug, Clone)]
pub struct RouteRegistration {
    /// The domain to register (e.g., "myproject.test").
    pub domain: String,

    /// The upstream host and port (e.g., "127.0.0.1:8080").
    pub target: String,

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
    let mut table = RouteTable::load(route_table_path)?;

    table.upsert(Route {
        domain: registration.domain.clone(),
        target: registration.target.clone(),
        source: registration.source,
        project: registration.project_path.clone(),
        tls: registration.tls,
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
) -> Result<(), GatewayError> {
    let mut table = RouteTable::load(route_table_path)?;

    // Ignore not-found errors — idempotent teardown.
    let _ = table.deregister(domain);

    table.save(route_table_path)?;
    Ok(())
}

/// Deregister all routes for a project path.
///
/// Useful when tearing down a container that may have registered
/// multiple domains (e.g., main domain + aliases).
pub fn deregister_project_routes(
    route_table_path: &Path,
    project_path: &str,
) -> Result<usize, GatewayError> {
    let mut table = RouteTable::load(route_table_path)?;

    let domains_to_remove: Vec<String> = table
        .all_routes()
        .iter()
        .filter(|r| r.project == project_path)
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
        target: format!("127.0.0.1:{port}"),
        tls,
        project_path: project_path.to_string(),
        source: RouteSource::Container,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::PortRegistry;

    #[test]
    fn register_and_deregister_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        let reg = RouteRegistration {
            domain: "myapp.test".to_string(),
            target: "127.0.0.1:8080".to_string(),
            tls: false,
            project_path: "/projects/myapp".to_string(),
            source: RouteSource::Container,
        };

        // Register.
        register_route(&path, &reg).unwrap();

        let table = RouteTable::load(&path).unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table.lookup("myapp.test").unwrap().target, "127.0.0.1:8080");

        // Deregister.
        deregister_route(&path, "myapp.test").unwrap();

        let table = RouteTable::load(&path).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn register_upserts_existing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        let reg1 = RouteRegistration {
            domain: "myapp.test".to_string(),
            target: "127.0.0.1:8080".to_string(),
            tls: false,
            project_path: "/projects/myapp".to_string(),
            source: RouteSource::Container,
        };
        register_route(&path, &reg1).unwrap();

        let reg2 = RouteRegistration {
            domain: "myapp.test".to_string(),
            target: "127.0.0.1:9090".to_string(),
            tls: true,
            project_path: "/projects/myapp".to_string(),
            source: RouteSource::Container,
        };
        register_route(&path, &reg2).unwrap();

        let table = RouteTable::load(&path).unwrap();
        assert_eq!(table.len(), 1);
        assert_eq!(table.lookup("myapp.test").unwrap().target, "127.0.0.1:9090");
        assert!(table.lookup("myapp.test").unwrap().tls);
    }

    #[test]
    fn deregister_nonexistent_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        // File doesn't exist yet — should still succeed.
        deregister_route(&path, "nonexistent.test").unwrap();
    }

    #[test]
    fn deregister_project_routes_removes_all() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        register_route(
            &path,
            &RouteRegistration {
                domain: "app.test".to_string(),
                target: "127.0.0.1:8080".to_string(),
                tls: false,
                project_path: "/projects/myapp".to_string(),
                source: RouteSource::Container,
            },
        )
        .unwrap();

        register_route(
            &path,
            &RouteRegistration {
                domain: "api.test".to_string(),
                target: "127.0.0.1:8080".to_string(),
                tls: false,
                project_path: "/projects/myapp".to_string(),
                source: RouteSource::Container,
            },
        )
        .unwrap();

        register_route(
            &path,
            &RouteRegistration {
                domain: "other.test".to_string(),
                target: "127.0.0.1:9090".to_string(),
                tls: false,
                project_path: "/projects/other".to_string(),
                source: RouteSource::Container,
            },
        )
        .unwrap();

        let count =
            deregister_project_routes(&path, "/projects/myapp").unwrap();
        assert_eq!(count, 2);

        let table = RouteTable::load(&path).unwrap();
        assert_eq!(table.len(), 1);
        assert!(table.lookup("other.test").is_some());
    }

    #[test]
    fn build_registration_with_default_port() {
        let reg = build_registration(
            "myapp.test",
            "myapp",
            "/projects/myapp",
            8080,
            false,
            None,
        );
        assert_eq!(reg.domain, "myapp.test");
        assert_eq!(reg.target, "127.0.0.1:8080");
        assert!(!reg.tls);
    }

    #[test]
    fn build_registration_with_port_registry() {
        let mut registry = PortRegistry::new();
        registry.allocate("myapp", "/projects/myapp");

        let reg = build_registration(
            "myapp.test",
            "myapp",
            "/projects/myapp",
            8080, // default, should be overridden
            false,
            Some(&registry),
        );
        // Should use the allocated port (8100), not the default (8080).
        assert_eq!(reg.target, "127.0.0.1:8100");
    }

    #[test]
    fn build_registration_with_tls() {
        let reg = build_registration(
            "myapp.dev",
            "myapp",
            "/projects/myapp",
            8080,
            true,
            None,
        );
        assert!(reg.tls);
    }
}
