//! Typed route snapshots; the gateway endpoint Store owns persistence.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::GatewayError;

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

    /// Process and listener generation that owns a dynamically managed host
    /// listener route. Static `target_host` routes leave this unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub managed_listener: Option<ManagedListenerRouteOwner>,

    /// Whether TLS is enabled for this route.
    #[serde(default)]
    pub tls: bool,

    /// When this route was registered.
    pub registered: DateTime<Utc>,
}

/// The recorded owner of a managed host listener route: the checkout path,
/// environment and process name in `owner`, the supervisor and child
/// generations that published it, and the reserved loopback address.
///
/// A listener is live while its address answers, so this record is a route
/// claim and nothing more. Unknown fields stay readable so a record written by
/// an earlier build is read once and rewritten without its retired identity
/// proofs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedListenerRouteOwner {
    /// Stable owner identity for one checkout/container/profile/process.
    pub owner: String,
    /// Container lifecycle generation that launched this supervisor.
    pub runtime_generation: String,
    /// Child restart generation.
    pub generation: String,
    /// Exact loopback address checked against the route target.
    pub address: String,
    /// Whether the public route terminates TLS at the gateway.
    pub route_tls: bool,
}

impl ManagedListenerRouteOwner {
    /// Whether the listener this route names is live. A managed listener is
    /// ready while its reserved port accepts a connection; nothing else is
    /// proved about the process behind it.
    pub fn answers(&self) -> bool {
        let Ok(address) = self.address.parse::<SocketAddr>() else {
            return false;
        };
        std::net::TcpStream::connect_timeout(&address, Duration::from_millis(300)).is_ok()
    }
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

/// The route portion of one committed endpoint snapshot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteTable {
    /// Kernel publication fence of this snapshot; zero for uncommitted fixtures.
    #[serde(skip)]
    pub revision: u64,
    /// All registered routes, keyed by domain.
    pub routes: HashMap<String, Route>,
}

impl RouteTable {
    /// Create an empty route table.
    pub fn new() -> Self {
        Self {
            revision: 0,
            routes: HashMap::new(),
        }
    }

    /// Observe the gateway endpoint Store.
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        crate::endpoint_store::EndpointStore::for_routes(path)
            .snapshot()?
            .routes()
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
        self.routes.get(domain).or_else(|| {
            self.routes
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(domain))
                .map(|(_, route)| route)
        })
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

/// Thread-safe route view backed by the gateway endpoint Store.
///
/// Used by DNS and proxy requests; a committed Store change triggers reload.
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
        let table = RouteTable::load(&path)?;
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
        let new_table = RouteTable::load(&self.path)?;
        let mut guard = crate::locks::write_tolerant(&self.table);
        *guard = new_table;
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

impl std::fmt::Debug for LiveRouteTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately do not lock or traverse the live route table: a
        // formatter must never block on the read-write lock or print live
        // route contents.
        f.debug_struct("LiveRouteTable")
            .field("path", &self.path)
            .field("table", &"<live route table>")
            .finish()
    }
}

#[cfg(test)]
#[path = "routes/tests.rs"]
mod tests;
