//! Typed route snapshots; the gateway endpoint Store owns persistence.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use fs2::FileExt;

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

    /// Generation token of the linked worktree that claimed this route.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

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

/// Kernel identity for a managed host listener route. Publication proves the
/// owner, and the proxy prepares verified connections in the background for
/// this exact route revision. Revocation guards also apply to reused sockets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManagedListenerRouteOwner {
    /// Stable owner identity for one checkout/container/profile/process.
    pub owner: String,
    /// Ignored legacy field: pre-0.15 owners recorded the publishing run. The
    /// owner is its checkout path, environment and process name, so this is
    /// dropped on the next write and always `None`.
    #[serde(default, skip_serializing)]
    pub run: Option<String>,
    /// Container lifecycle generation that launched this supervisor.
    pub runtime_generation: String,
    /// Child restart generation.
    pub generation: String,
    /// Detached Effigy supervisor PID and exact generation identity.
    pub supervisor_pid: u32,
    pub supervisor_boot_identity: String,
    pub supervisor_start_identity: crate::identity::GatewayStartIdentity,
    /// Recorded managed shell leader PID and exact generation identity.
    pub root_pid: u32,
    pub root_boot_identity: String,
    pub root_start_identity: crate::identity::GatewayStartIdentity,
    /// Process that owns the listening socket and exact generation identity.
    pub listener_pid: u32,
    pub listener_boot_identity: String,
    pub listener_start_identity: crate::identity::GatewayStartIdentity,
    /// Exact loopback address checked against the route target.
    pub address: String,
    /// Whether the public route terminates TLS at the gateway.
    pub route_tls: bool,
    /// Generation state file that makes a cached route unavailable before
    /// its child group is stopped or replaced.
    pub availability_file: String,
}

/// Process-wide and cross-process lock for route-table read/check/write.
/// The lock file is separate from the atomically replaced JSON file.
pub struct RouteTableLock(std::fs::File);

impl RouteTableLock {
    pub fn acquire(path: &Path) -> Result<Self, GatewayError> {
        Self::acquire_until(path, Instant::now() + Duration::from_secs(30))
    }
    pub(crate) fn acquire_until(path: &Path, deadline: Instant) -> Result<Self, GatewayError> {
        let lock_path = path.with_extension("lock");
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                GatewayError::RouteTableWriteError {
                    path: lock_path.clone(),
                    reason: error.to_string(),
                }
            })?;
        }
        #[cfg(unix)]
        let file = {
            use std::os::unix::fs::OpenOptionsExt;
            OpenOptions::new()
                .create(true)
                .truncate(false)
                .read(true)
                .write(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&lock_path)
        };
        #[cfg(not(unix))]
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path);
        let file = file.map_err(|error| GatewayError::RouteTableWriteError {
            path: lock_path.clone(),
            reason: error.to_string(),
        })?;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => break,
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(
                        Duration::from_millis(10)
                            .min(deadline.saturating_duration_since(Instant::now())),
                    );
                }
                Err(e) => {
                    return Err(GatewayError::RouteTableWriteError {
                        path: lock_path,
                        reason: format!("lock refused or deadline expired: {e}"),
                    })
                }
            }
        }
        Ok(Self(file))
    }
}

impl Drop for RouteTableLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

impl std::fmt::Debug for RouteTableLock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The newtype owns a lock file; the raw descriptor is an
        // implementation detail and is redacted from diagnostics.
        f.debug_struct("RouteTableLock").finish_non_exhaustive()
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
