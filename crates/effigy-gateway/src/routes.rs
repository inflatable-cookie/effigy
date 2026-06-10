//! Route table management for the gateway.
//!
//! The route table is a JSON file at `~/.effigy/gateway/routes.json` that
//! maps domains to upstream targets. It's the coordination layer between
//! container lifecycle events (which register/deregister routes) and the
//! gateway (which reads routes for DNS and proxy decisions).
//!
//! Concurrent access is handled via atomic file replacement (write to
//! temp file, then rename). The gateway watches for changes via filesystem
//! notifications.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::atomic_write;
use crate::error::GatewayError;

/// Provenance marker Effigy stamps into the route table envelope on save.
///
/// The gateway's read-path trust check (see [`crate::trust`]) requires this
/// marker before an elevated daemon will trust the file. See
/// `docs/contracts/033-gateway-route-table-trust-contract.md`.
pub const ROUTE_TABLE_MANAGED_MARKER: &str = "effigy-gateway-route-table-v1";

/// On-disk envelope: routes plus the Effigy-managed provenance marker.
///
/// Serialized on save so the trust check can confirm Effigy wrote the file.
/// `RouteTable` itself only models `routes`; the marker is read separately by
/// the trust inspector, and `RouteTable::load` ignores it as an unknown field.
#[derive(Serialize)]
struct ManagedRouteTableRef<'a> {
    #[serde(rename = "_managed_by")]
    managed_by: &'static str,
    routes: &'a HashMap<String, Route>,
}

/// Restrict the route table to owner-only access so a non-owner local user
/// cannot tamper with the table the (possibly elevated) gateway daemon trusts.
#[cfg(unix)]
fn set_owner_only_permissions(path: &Path) -> Result<(), GatewayError> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(|e| {
        GatewayError::RouteTableWriteError {
            path: path.to_path_buf(),
            reason: format!("failed to set owner-only permissions: {e}"),
        }
    })
}

#[cfg(not(unix))]
fn set_owner_only_permissions(_path: &Path) -> Result<(), GatewayError> {
    Ok(())
}

/// A single route entry mapping a domain to an upstream target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Route {
    /// The domain name (e.g., "myproject.test").
    pub domain: String,

    /// Optional upstream target (e.g., "127.0.0.1:8080").
    ///
    /// DNS-only TCP service aliases intentionally leave this unset so the DNS
    /// layer can resolve them without making the HTTP proxy treat them as an
    /// upstream route.
    #[serde(default)]
    pub target: Option<String>,

    /// Optional DNS IP override for this route.
    ///
    /// When present, the DNS resolver answers with this IP instead of the
    /// gateway-wide default `resolve_to` address. This keeps DNS behavior
    /// route-specific without changing the HTTP proxy contract.
    #[serde(default)]
    pub dns_ip: Option<Ipv4Addr>,

    /// Optional TCP bind port for a DNS-only service alias listener owned by
    /// the host gateway.
    #[serde(default)]
    pub tcp_port: Option<u16>,

    /// Optional upstream target for a DNS-only service alias listener.
    #[serde(default)]
    pub tcp_target: Option<String>,

    /// How this route was registered.
    pub source: RouteSource,

    /// Absolute path to the project directory.
    pub project: String,

    /// Whether TLS is enabled for this route.
    #[serde(default)]
    pub tls: bool,

    /// When this route was registered.
    pub registered: DateTime<Utc>,
}

/// How a route was registered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RouteSource {
    /// Registered by a container lifecycle event.
    Container,
    /// Registered by a task lifecycle event (non-container project).
    Task,
    /// Registered manually.
    Manual,
}

/// The route table — serialized to/from JSON on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTable {
    /// All registered routes, keyed by domain.
    pub routes: HashMap<String, Route>,
}

impl RouteTable {
    /// Create an empty route table.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
        }
    }

    /// Load a route table from a JSON file.
    ///
    /// If the file doesn't exist, returns an empty table.
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content =
            std::fs::read_to_string(path).map_err(|e| GatewayError::RouteTableReadError {
                path: path.to_path_buf(),
                reason: e.to_string(),
            })?;

        serde_json::from_str(&content).map_err(|e| GatewayError::RouteTableParseError {
            path: path.to_path_buf(),
            reason: e.to_string(),
        })
    }

    /// Save the route table to a JSON file atomically.
    ///
    /// Writes to a temporary file first, then renames. This prevents
    /// partial reads by the gateway file watcher.
    pub fn save(&self, path: &Path) -> Result<(), GatewayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("failed to create directory: {e}"),
            })?;
        }

        let envelope = ManagedRouteTableRef {
            managed_by: ROUTE_TABLE_MANAGED_MARKER,
            routes: &self.routes,
        };
        let content = serde_json::to_string_pretty(&envelope).map_err(|e| {
            GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("failed to serialize: {e}"),
            }
        })?;

        // Write to a temp file in the same directory, then rename.
        // This ensures atomic replacement.
        let temp_path = atomic_write::temp_path(path, "routes");
        std::fs::write(&temp_path, &content).map_err(|e| GatewayError::RouteTableWriteError {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

        // Set owner-only permissions on the temp file before the rename so the
        // published file is never briefly group/other-writable.
        set_owner_only_permissions(&temp_path)?;

        std::fs::rename(&temp_path, path).map_err(|e| GatewayError::RouteTableWriteError {
            path: path.to_path_buf(),
            reason: format!("atomic rename failed: {e}"),
        })?;

        Ok(())
    }

    /// Register a new route.
    ///
    /// Returns an error if a route for the domain already exists.
    pub fn register(&mut self, route: Route) -> Result<(), GatewayError> {
        if self.routes.contains_key(&route.domain) {
            return Err(GatewayError::DuplicateRoute {
                domain: route.domain,
            });
        }
        self.routes.insert(route.domain.clone(), route);
        Ok(())
    }

    /// Update an existing route or insert a new one.
    pub fn upsert(&mut self, route: Route) {
        self.routes.insert(route.domain.clone(), route);
    }

    /// Remove a route by domain.
    ///
    /// Returns the removed route, or an error if not found.
    pub fn deregister(&mut self, domain: &str) -> Result<Route, GatewayError> {
        self.routes
            .remove(domain)
            .ok_or_else(|| GatewayError::RouteNotFound {
                domain: domain.to_string(),
            })
    }

    /// Look up a route by domain.
    pub fn lookup(&self, domain: &str) -> Option<&Route> {
        self.routes.get(domain)
    }

    /// All registered routes, sorted by domain.
    pub fn all_routes(&self) -> Vec<&Route> {
        let mut routes: Vec<_> = self.routes.values().collect();
        routes.sort_by_key(|r| &r.domain);
        routes
    }

    /// Number of registered routes.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }
}

impl Default for RouteTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe, live-reloading route table backed by a JSON file.
///
/// Used by the running gateway to serve DNS and proxy requests. The table
/// is reloaded from disk when a filesystem change notification fires.
pub struct LiveRouteTable {
    /// Path to the route table JSON file.
    path: PathBuf,

    /// The in-memory route table, behind a read-write lock.
    table: Arc<RwLock<RouteTable>>,
}

impl LiveRouteTable {
    /// Create a new live route table from the given file path.
    ///
    /// Loads the current contents from disk. If the file doesn't exist,
    /// starts with an empty table.
    pub fn new(path: PathBuf) -> Result<Self, GatewayError> {
        // Enforce the read-path trust gate (contract 033). An untrusted file at
        // startup yields an empty table — there is no last-known-good yet.
        let table = crate::trust::load_trusted(&path)?.unwrap_or_default();
        Ok(Self {
            path,
            table: Arc::new(RwLock::new(table)),
        })
    }

    /// Get a read handle to the route table.
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, RouteTable> {
        crate::locks::read_tolerant(&self.table)
    }

    /// Reload the route table from disk.
    ///
    /// Called by the file watcher when the route table file changes.
    pub fn reload(&self) -> Result<(), GatewayError> {
        // Untrusted file: keep the last-known-good in-memory table (contract 033).
        if let Some(new_table) = crate::trust::load_trusted(&self.path)? {
            let mut guard = crate::locks::write_tolerant(&self.table);
            *guard = new_table;
        }
        Ok(())
    }

    /// Get a clone of the Arc for sharing across async tasks.
    pub fn shared_table(&self) -> Arc<RwLock<RouteTable>> {
        Arc::clone(&self.table)
    }

    /// Path to the underlying file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
#[path = "routes/tests.rs"]
mod tests;
