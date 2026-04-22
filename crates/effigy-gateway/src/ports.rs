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

    /// Stable service+container-port to host-port assignments within this range.
    #[serde(default)]
    pub assigned_ports: HashMap<String, u16>,
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
    fn assigned_port_key(service_name: &str, container_port: u16) -> String {
        format!("{service_name}:{container_port}")
    }

    fn legacy_port_key(container_port: u16) -> String {
        container_port.to_string()
    }

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

        let content =
            std::fs::read_to_string(path).map_err(|e| GatewayError::RouteTableReadError {
                path: path.to_path_buf(),
                reason: format!("port registry: {e}"),
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

        let content =
            serde_json::to_string_pretty(self).map_err(|e| GatewayError::RouteTableWriteError {
                path: path.to_path_buf(),
                reason: format!("port registry serialize: {e}"),
            })?;

        let temp_path = path.with_extension(format!(
            "json.tmp.{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|value| value.as_nanos())
                .unwrap_or_default()
        ));
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
    pub fn allocate(&mut self, project_name: &str, project_path: &str) -> &PortAllocation {
        if self.allocations.contains_key(project_name) {
            return &self.allocations[project_name];
        }

        let base = self.next_available_base(DEFAULT_RANGE);
        let alloc = PortAllocation {
            base,
            range: DEFAULT_RANGE,
            project: project_path.to_string(),
            assigned_ports: HashMap::new(),
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
            assigned_ports: HashMap::new(),
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

        self.allocations.insert(project_name.to_string(), proposed);
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
            http: alloc
                .assigned_ports
                .get("80")
                .copied()
                .unwrap_or_else(|| alloc.port_for(ServicePortOffsets::HTTP)),
            mysql: alloc
                .assigned_ports
                .get("3306")
                .copied()
                .unwrap_or_else(|| alloc.port_for(ServicePortOffsets::MYSQL)),
            postgres: alloc
                .assigned_ports
                .get("5432")
                .copied()
                .unwrap_or_else(|| alloc.port_for(ServicePortOffsets::POSTGRES)),
            redis: alloc
                .assigned_ports
                .get("6379")
                .copied()
                .unwrap_or_else(|| alloc.port_for(ServicePortOffsets::REDIS)),
            memcached: alloc
                .assigned_ports
                .get("11211")
                .copied()
                .unwrap_or_else(|| alloc.port_for(ServicePortOffsets::MEMCACHED)),
            base: alloc.base,
        })
    }

    /// Resolve a stable host port for one exposed container port.
    pub fn assign_port(
        &mut self,
        project_name: &str,
        project_path: &str,
        service_name: &str,
        container_port: u16,
    ) -> Result<u16, GatewayError> {
        if !self.allocations.contains_key(project_name) {
            let _ = self.allocate(project_name, project_path);
        }
        let allocation = self
            .allocations
            .get_mut(project_name)
            .expect("allocation should exist");

        let binding_key = Self::assigned_port_key(service_name, container_port);
        if let Some(host_port) = allocation.assigned_ports.get(&binding_key) {
            return Ok(*host_port);
        }

        let legacy_key = Self::legacy_port_key(container_port);
        if let Some(host_port) = allocation.assigned_ports.remove(&legacy_key) {
            allocation
                .assigned_ports
                .insert(binding_key.clone(), host_port);
            return Ok(host_port);
        }

        if let Some(offset) = preferred_offset_for(container_port) {
            let host_port = allocation.port_for(offset);
            if !allocation
                .assigned_ports
                .values()
                .any(|value| *value == host_port)
            {
                allocation.assigned_ports.insert(binding_key, host_port);
                return Ok(host_port);
            }
        }

        let host_port = (allocation.base..allocation.end())
            .find(|port| {
                !allocation
                    .assigned_ports
                    .values()
                    .any(|value| value == port)
            })
            .ok_or_else(|| GatewayError::RouteTableWriteError {
                path: Path::new("~/.effigy/ports.json").to_path_buf(),
                reason: format!(
                    "port registry: project `{project_name}` exhausted allocated range {}..{}",
                    allocation.base,
                    allocation.end()
                ),
            })?;
        allocation.assigned_ports.insert(binding_key, host_port);
        Ok(host_port)
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

fn preferred_offset_for(container_port: u16) -> Option<u16> {
    match container_port {
        80 => Some(ServicePortOffsets::HTTP),
        1025 => Some(26),
        3000 => Some(30),
        3306 => Some(ServicePortOffsets::MYSQL),
        5432 => Some(ServicePortOffsets::POSTGRES),
        6379 => Some(ServicePortOffsets::REDIS),
        8025 => Some(25),
        9000 => Some(40),
        9001 => Some(41),
        9200 => Some(20),
        11211 => Some(ServicePortOffsets::MEMCACHED),
        _ => None,
    }
}

#[cfg(test)]
#[path = "ports/tests.rs"]
mod tests;
