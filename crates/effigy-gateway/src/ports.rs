//! Port allocation registry for multi-project coordination.
//!
//! When multiple projects use effigy containers simultaneously, port
//! conflicts are a common problem (every project wants port 8080 and 3306).
//! This module maintains a persistent port allocation registry at
//! `~/.effigy/ports.json` that assigns non-conflicting port ranges to
//! projects.
//!
//! ## Port ranges
//!
//! Each project gets a base port and a range size. Services within that
//! project are allocated ports starting from the base. For example:
//!
//! - Project A: base 8100, range 100 → ports 8100–8199
//! - Project B: base 8200, range 100 → ports 8200–8299
//!
//! The standard service port offsets within a range:
//!
//! | Offset | Service     |
//! |--------|-------------|
//! | +0     | HTTP/nginx  |
//! | +6     | MySQL/MariaDB (3306 → base+6) |
//! | +32    | PostgreSQL  (5432 → base+32) |
//! | +79    | Redis       (6379 → base+79) |
//! | +11    | Memcached   (11211 → base+11) |

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::GatewayError;

/// Default port range start for auto-allocation.
const DEFAULT_BASE: u16 = 8100;

/// Default range size per project.
const DEFAULT_RANGE: u16 = 100;

/// Standard service port offsets within a project's allocated range.
#[derive(Debug, Clone, Copy)]
pub struct ServicePortOffsets;

impl ServicePortOffsets {
    /// HTTP (nginx) — offset 0 from base.
    pub const HTTP: u16 = 0;
    /// MySQL/MariaDB — offset 6 (like 3306 → X06).
    pub const MYSQL: u16 = 6;
    /// PostgreSQL — offset 32 (like 5432 → X32).
    pub const POSTGRES: u16 = 32;
    /// Redis — offset 79 (like 6379 → X79).
    pub const REDIS: u16 = 79;
    /// Memcached — offset 11 (like 11211 → X11).
    pub const MEMCACHED: u16 = 11;
}

/// A port allocation for a single project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PortAllocation {
    /// Base port for this project.
    pub base: u16,

    /// Number of ports allocated (e.g., 100).
    pub range: u16,

    /// Absolute path to the project directory.
    pub project: String,
}

impl PortAllocation {
    /// Get the allocated port for a service by offset.
    pub fn port_for(&self, offset: u16) -> u16 {
        self.base + offset
    }

    /// The last port in this allocation's range (exclusive).
    pub fn end(&self) -> u16 {
        self.base + self.range
    }

    /// Check if a port falls within this allocation.
    pub fn contains(&self, port: u16) -> bool {
        port >= self.base && port < self.end()
    }

    /// Check if this allocation overlaps with another.
    pub fn overlaps(&self, other: &PortAllocation) -> bool {
        self.base < other.end() && other.base < self.end()
    }
}

/// The port allocation registry — persisted to `~/.effigy/ports.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRegistry {
    /// Allocations keyed by project name.
    pub allocations: HashMap<String, PortAllocation>,
}

impl PortRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            allocations: HashMap::new(),
        }
    }

    /// Load the registry from a JSON file.
    ///
    /// Returns an empty registry if the file doesn't exist.
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content = std::fs::read_to_string(path).map_err(|e| {
            GatewayError::RouteTableReadError {
                path: path.to_path_buf(),
                reason: format!("port registry: {e}"),
            }
        })?;

        serde_json::from_str(&content).map_err(|e| GatewayError::RouteTableParseError {
            path: path.to_path_buf(),
            reason: format!("port registry: {e}"),
        })
    }

    /// Save the registry to a JSON file atomically.
    pub fn save(&self, path: &Path) -> Result<(), GatewayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("port registry: {e}"),
            })?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| {
            GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("port registry serialize: {e}"),
            }
        })?;

        let temp_path = path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content).map_err(|e| GatewayError::RouteTableWriteError {
            path: temp_path.clone(),
            reason: e.to_string(),
        })?;

        std::fs::rename(&temp_path, path).map_err(|e| GatewayError::RouteTableWriteError {
            path: path.to_path_buf(),
            reason: format!("atomic rename: {e}"),
        })?;

        Ok(())
    }

    /// Get the allocation for a project, if registered.
    pub fn get(&self, project_name: &str) -> Option<&PortAllocation> {
        self.allocations.get(project_name)
    }

    /// Allocate a port range for a project.
    ///
    /// If the project already has an allocation, returns it unchanged.
    /// Otherwise, finds the next available base port and allocates a range.
    pub fn allocate(
        &mut self,
        project_name: &str,
        project_path: &str,
    ) -> &PortAllocation {
        if self.allocations.contains_key(project_name) {
            return &self.allocations[project_name];
        }

        let base = self.next_available_base(DEFAULT_RANGE);
        let alloc = PortAllocation {
            base,
            range: DEFAULT_RANGE,
            project: project_path.to_string(),
        };
        self.allocations.insert(project_name.to_string(), alloc);
        &self.allocations[project_name]
    }

    /// Allocate with a specific base port.
    ///
    /// Returns an error if the range conflicts with an existing allocation.
    pub fn allocate_at(
        &mut self,
        project_name: &str,
        project_path: &str,
        base: u16,
        range: u16,
    ) -> Result<&PortAllocation, GatewayError> {
        let proposed = PortAllocation {
            base,
            range,
            project: project_path.to_string(),
        };

        // Check for conflicts.
        for (name, existing) in &self.allocations {
            if name == project_name {
                continue;
            }
            if proposed.overlaps(existing) {
                return Err(GatewayError::PortConflict {
                    project: project_name.to_string(),
                    conflicting_project: name.clone(),
                    base,
                    range,
                });
            }
        }

        self.allocations
            .insert(project_name.to_string(), proposed);
        Ok(&self.allocations[project_name])
    }

    /// Remove a project's allocation.
    pub fn deallocate(&mut self, project_name: &str) -> Option<PortAllocation> {
        self.allocations.remove(project_name)
    }

    /// Find the next available base port that doesn't conflict with any
    /// existing allocation.
    fn next_available_base(&self, range: u16) -> u16 {
        if self.allocations.is_empty() {
            return DEFAULT_BASE;
        }

        // Collect all allocated ranges and sort by base.
        let mut ranges: Vec<(u16, u16)> = self
            .allocations
            .values()
            .map(|a| (a.base, a.end()))
            .collect();
        ranges.sort_by_key(|&(base, _)| base);

        // Try to fit before the first range.
        if ranges[0].0 >= DEFAULT_BASE + range {
            return DEFAULT_BASE;
        }

        // Try to fit between ranges.
        for window in ranges.windows(2) {
            let gap_start = window[0].1;
            let gap_end = window[1].0;
            if gap_end - gap_start >= range {
                return gap_start;
            }
        }

        // Fit after the last range.
        ranges.last().unwrap().1
    }

    /// Generate a port mapping table for a project.
    ///
    /// Returns a map of service type → allocated host port.
    pub fn port_map(&self, project_name: &str) -> Option<PortMap> {
        self.get(project_name).map(|alloc| PortMap {
            http: alloc.port_for(ServicePortOffsets::HTTP),
            mysql: alloc.port_for(ServicePortOffsets::MYSQL),
            postgres: alloc.port_for(ServicePortOffsets::POSTGRES),
            redis: alloc.port_for(ServicePortOffsets::REDIS),
            memcached: alloc.port_for(ServicePortOffsets::MEMCACHED),
            base: alloc.base,
        })
    }

    /// Number of registered allocations.
    pub fn len(&self) -> usize {
        self.allocations.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.allocations.is_empty()
    }
}

impl Default for PortRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard port assignments for a project.
#[derive(Debug, Clone, Copy)]
pub struct PortMap {
    /// HTTP port.
    pub http: u16,
    /// MySQL/MariaDB port.
    pub mysql: u16,
    /// PostgreSQL port.
    pub postgres: u16,
    /// Redis port.
    pub redis: u16,
    /// Memcached port.
    pub memcached: u16,
    /// Base port for custom services.
    pub base: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── PortAllocation ───────────────────────────────────────────────

    #[test]
    fn allocation_contains_ports_in_range() {
        let alloc = PortAllocation {
            base: 8100,
            range: 100,
            project: "/tmp".to_string(),
        };
        assert!(alloc.contains(8100));
        assert!(alloc.contains(8150));
        assert!(alloc.contains(8199));
        assert!(!alloc.contains(8200));
        assert!(!alloc.contains(8099));
    }

    #[test]
    fn allocation_overlap_detection() {
        let a = PortAllocation {
            base: 8100,
            range: 100,
            project: "/a".to_string(),
        };
        let b = PortAllocation {
            base: 8150,
            range: 100,
            project: "/b".to_string(),
        };
        let c = PortAllocation {
            base: 8200,
            range: 100,
            project: "/c".to_string(),
        };
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        assert!(!a.overlaps(&c));
        assert!(!c.overlaps(&a));
    }

    #[test]
    fn allocation_port_for_offset() {
        let alloc = PortAllocation {
            base: 8200,
            range: 100,
            project: "/tmp".to_string(),
        };
        assert_eq!(alloc.port_for(ServicePortOffsets::HTTP), 8200);
        assert_eq!(alloc.port_for(ServicePortOffsets::MYSQL), 8206);
        assert_eq!(alloc.port_for(ServicePortOffsets::POSTGRES), 8232);
        assert_eq!(alloc.port_for(ServicePortOffsets::REDIS), 8279);
    }

    // ── PortRegistry ─────────────────────────────────────────────────

    #[test]
    fn empty_registry() {
        let reg = PortRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn auto_allocate_first_project() {
        let mut reg = PortRegistry::new();
        let alloc = reg.allocate("client-a", "/projects/a");
        assert_eq!(alloc.base, DEFAULT_BASE);
        assert_eq!(alloc.range, DEFAULT_RANGE);
    }

    #[test]
    fn auto_allocate_second_project_no_overlap() {
        let mut reg = PortRegistry::new();
        reg.allocate("client-a", "/projects/a");
        let alloc_b = reg.allocate("client-b", "/projects/b");
        assert_eq!(alloc_b.base, DEFAULT_BASE + DEFAULT_RANGE);

        let a = reg.get("client-a").unwrap();
        let b = reg.get("client-b").unwrap();
        assert!(!a.overlaps(b));
    }

    #[test]
    fn auto_allocate_idempotent() {
        let mut reg = PortRegistry::new();
        let base1 = reg.allocate("client-a", "/projects/a").base;
        let base2 = reg.allocate("client-a", "/projects/a").base;
        assert_eq!(base1, base2);
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn allocate_at_specific_base() {
        let mut reg = PortRegistry::new();
        reg.allocate_at("client-a", "/a", 9000, 50).unwrap();
        let alloc = reg.get("client-a").unwrap();
        assert_eq!(alloc.base, 9000);
        assert_eq!(alloc.range, 50);
    }

    #[test]
    fn allocate_at_conflicts_detected() {
        let mut reg = PortRegistry::new();
        reg.allocate_at("client-a", "/a", 8100, 100).unwrap();

        let result = reg.allocate_at("client-b", "/b", 8150, 100);
        assert!(result.is_err());
    }

    #[test]
    fn allocate_at_adjacent_no_conflict() {
        let mut reg = PortRegistry::new();
        reg.allocate_at("client-a", "/a", 8100, 100).unwrap();
        reg.allocate_at("client-b", "/b", 8200, 100).unwrap();

        assert_eq!(reg.len(), 2);
        let a = reg.get("client-a").unwrap();
        let b = reg.get("client-b").unwrap();
        assert!(!a.overlaps(b));
    }

    #[test]
    fn deallocate_removes_project() {
        let mut reg = PortRegistry::new();
        reg.allocate("client-a", "/a");
        assert_eq!(reg.len(), 1);

        let removed = reg.deallocate("client-a");
        assert!(removed.is_some());
        assert!(reg.is_empty());
    }

    #[test]
    fn deallocate_nonexistent_returns_none() {
        let mut reg = PortRegistry::new();
        assert!(reg.deallocate("nonexistent").is_none());
    }

    #[test]
    fn auto_allocate_fills_gaps() {
        let mut reg = PortRegistry::new();
        reg.allocate("a", "/a");  // 8100-8199
        reg.allocate("b", "/b");  // 8200-8299
        reg.allocate("c", "/c");  // 8300-8399
        reg.deallocate("b");       // Free 8200-8299

        let d = reg.allocate("d", "/d");
        assert_eq!(d.base, 8200); // Should fill the gap.
    }

    #[test]
    fn port_map_generation() {
        let mut reg = PortRegistry::new();
        reg.allocate("client", "/projects/client");

        let map = reg.port_map("client").unwrap();
        assert_eq!(map.http, DEFAULT_BASE);
        assert_eq!(map.mysql, DEFAULT_BASE + 6);
        assert_eq!(map.postgres, DEFAULT_BASE + 32);
        assert_eq!(map.redis, DEFAULT_BASE + 79);
        assert_eq!(map.memcached, DEFAULT_BASE + 11);
    }

    #[test]
    fn port_map_nonexistent_returns_none() {
        let reg = PortRegistry::new();
        assert!(reg.port_map("nonexistent").is_none());
    }

    // ── Persistence ──────────────────────────────────────────────────

    #[test]
    fn save_and_load_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ports.json");

        let mut reg = PortRegistry::new();
        reg.allocate("client-a", "/projects/a");
        reg.allocate("client-b", "/projects/b");
        reg.save(&path).unwrap();

        let loaded = PortRegistry::load(&path).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(
            loaded.get("client-a").unwrap().base,
            reg.get("client-a").unwrap().base,
        );
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let reg = PortRegistry::load(Path::new("/nonexistent/ports.json")).unwrap();
        assert!(reg.is_empty());
    }
}
