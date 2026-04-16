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
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::error::GatewayError;

/// A single route entry mapping a domain to an upstream target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Route {
    /// The domain name (e.g., "myproject.test").
    pub domain: String,

    /// Upstream target (e.g., "127.0.0.1:8080").
    pub target: String,

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

        let content =
            serde_json::to_string_pretty(self).map_err(|e| GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("failed to serialize: {e}"),
            })?;

        // Write to a temp file in the same directory, then rename.
        // This ensures atomic replacement.
        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content).map_err(|e| GatewayError::RouteTableWriteError {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

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
        let table = RouteTable::load(&path)?;
        Ok(Self {
            path,
            table: Arc::new(RwLock::new(table)),
        })
    }

    /// Get a read handle to the route table.
    pub fn read(&self) -> std::sync::RwLockReadGuard<'_, RouteTable> {
        self.table.read().expect("route table lock poisoned")
    }

    /// Reload the route table from disk.
    ///
    /// Called by the file watcher when the route table file changes.
    pub fn reload(&self) -> Result<(), GatewayError> {
        let new_table = RouteTable::load(&self.path)?;
        let mut guard = self.table.write().expect("route table lock poisoned");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_route(domain: &str, target: &str) -> Route {
        Route {
            domain: domain.to_string(),
            target: target.to_string(),
            source: RouteSource::Container,
            project: "/tmp/test".to_string(),
            tls: false,
            registered: Utc::now(),
        }
    }

    #[test]
    fn empty_table() {
        let table = RouteTable::new();
        assert!(table.is_empty());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn register_and_lookup() {
        let mut table = RouteTable::new();
        let route = test_route("myapp.test", "127.0.0.1:8080");
        table.register(route).unwrap();

        assert_eq!(table.len(), 1);
        let found = table.lookup("myapp.test").unwrap();
        assert_eq!(found.target, "127.0.0.1:8080");
    }

    #[test]
    fn duplicate_registration_fails() {
        let mut table = RouteTable::new();
        table
            .register(test_route("myapp.test", "127.0.0.1:8080"))
            .unwrap();

        let result = table.register(test_route("myapp.test", "127.0.0.1:9090"));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::DuplicateRoute { .. }
        ));
    }

    #[test]
    fn upsert_replaces_existing() {
        let mut table = RouteTable::new();
        table.upsert(test_route("myapp.test", "127.0.0.1:8080"));
        table.upsert(test_route("myapp.test", "127.0.0.1:9090"));

        assert_eq!(table.len(), 1);
        assert_eq!(table.lookup("myapp.test").unwrap().target, "127.0.0.1:9090");
    }

    #[test]
    fn deregister_removes_route() {
        let mut table = RouteTable::new();
        table
            .register(test_route("myapp.test", "127.0.0.1:8080"))
            .unwrap();

        let removed = table.deregister("myapp.test").unwrap();
        assert_eq!(removed.target, "127.0.0.1:8080");
        assert!(table.is_empty());
    }

    #[test]
    fn deregister_nonexistent_fails() {
        let mut table = RouteTable::new();
        let result = table.deregister("nonexistent.test");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            GatewayError::RouteNotFound { .. }
        ));
    }

    #[test]
    fn all_routes_sorted() {
        let mut table = RouteTable::new();
        table.upsert(test_route("zzz.test", "127.0.0.1:1"));
        table.upsert(test_route("aaa.test", "127.0.0.1:2"));
        table.upsert(test_route("mmm.test", "127.0.0.1:3"));

        let routes = table.all_routes();
        assert_eq!(routes[0].domain, "aaa.test");
        assert_eq!(routes[1].domain, "mmm.test");
        assert_eq!(routes[2].domain, "zzz.test");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.upsert(test_route("api.test", "127.0.0.1:3000"));
        table.save(&path).unwrap();

        let loaded = RouteTable::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded.lookup("app.test").unwrap().target, "127.0.0.1:8080");
        assert_eq!(loaded.lookup("api.test").unwrap().target, "127.0.0.1:3000");
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let table = RouteTable::load(Path::new("/nonexistent/routes.json")).unwrap();
        assert!(table.is_empty());
    }

    #[test]
    fn save_creates_parent_directories() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep/nested/dir/routes.json");

        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.save(&path).unwrap();

        assert!(path.exists());
    }

    #[test]
    fn save_is_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        // Write initial.
        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.save(&path).unwrap();

        // Overwrite.
        table.upsert(test_route("app.test", "127.0.0.1:9090"));
        table.save(&path).unwrap();

        // Temp file should not remain.
        assert!(!dir.path().join("routes.json.tmp").exists());

        // Should have the latest value.
        let loaded = RouteTable::load(&path).unwrap();
        assert_eq!(loaded.lookup("app.test").unwrap().target, "127.0.0.1:9090");
    }

    #[test]
    fn json_format_is_human_readable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.save(&path).unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        // Pretty-printed JSON should have newlines and indentation.
        assert!(content.contains('\n'));
        assert!(content.contains("  "));
        assert!(content.contains("\"domain\""));
        assert!(content.contains("\"target\""));
    }

    #[test]
    fn route_serialization_format() {
        let route = Route {
            domain: "test.test".to_string(),
            target: "127.0.0.1:8080".to_string(),
            source: RouteSource::Container,
            project: "/tmp/proj".to_string(),
            tls: false,
            registered: DateTime::parse_from_rfc3339("2026-04-16T10:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        };

        let json = serde_json::to_string_pretty(&route).unwrap();
        assert!(json.contains("\"source\": \"container\""));
        assert!(json.contains("\"tls\": false"));

        // Deserialize back.
        let parsed: Route = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.domain, "test.test");
        assert_eq!(parsed.source, RouteSource::Container);
    }

    #[test]
    fn live_route_table_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.save(&path).unwrap();

        let live = LiveRouteTable::new(path).unwrap();
        let guard = live.read();
        assert_eq!(guard.len(), 1);
        assert_eq!(guard.lookup("app.test").unwrap().target, "127.0.0.1:8080");
    }

    #[test]
    fn live_route_table_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("routes.json");

        // Write initial.
        let mut table = RouteTable::new();
        table.upsert(test_route("app.test", "127.0.0.1:8080"));
        table.save(&path).unwrap();

        let live = LiveRouteTable::new(path.clone()).unwrap();
        assert_eq!(live.read().len(), 1);

        // Write updated table externally (simulating another process).
        let mut updated = RouteTable::new();
        updated.upsert(test_route("app.test", "127.0.0.1:9090"));
        updated.upsert(test_route("api.test", "127.0.0.1:3000"));
        updated.save(&path).unwrap();

        // Reload.
        live.reload().unwrap();
        let guard = live.read();
        assert_eq!(guard.len(), 2);
        assert_eq!(guard.lookup("app.test").unwrap().target, "127.0.0.1:9090");
    }
}
