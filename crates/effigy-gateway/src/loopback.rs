//! Stable loopback-IP allocation for gateway-owned route groups.
//!
//! TCP service DNS will later bind project or shared-service identities onto a
//! stable IP in the bounded `127.1.0.x` pool. This registry persists those
//! assignments under gateway state so later registration work can reuse them
//! without elevated privileges during normal runtime.

use std::collections::HashMap;
use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::atomic_write;
use crate::error::GatewayError;

/// First assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_START: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 1);

/// Last assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_END: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 50);

/// One persisted loopback-IP assignment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopbackAssignment {
    /// Stable IP reserved for the identity.
    pub ip: Ipv4Addr,

    /// Owning path or other bounded scope label for the identity.
    pub scope: String,
}

/// Persistent loopback-IP registry keyed by project or shared-service identity.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LoopbackRegistry {
    /// Assignments keyed by stable route-group identity.
    pub assignments: HashMap<String, LoopbackAssignment>,
}

impl LoopbackRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            assignments: HashMap::new(),
        }
    }

    /// Load the registry from a JSON file.
    ///
    /// Returns an empty registry if the file does not exist yet.
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        if !path.exists() {
            return Ok(Self::new());
        }

        let content =
            std::fs::read_to_string(path).map_err(|error| GatewayError::LoopbackRegistryRead {
                path: path.to_path_buf(),
                reason: error.to_string(),
            })?;

        serde_json::from_str(&content).map_err(|error| GatewayError::LoopbackRegistryParse {
            path: path.to_path_buf(),
            reason: error.to_string(),
        })
    }

    /// Save the registry to a JSON file atomically.
    pub fn save(&self, path: &Path) -> Result<(), GatewayError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                GatewayError::LoopbackRegistryWrite {
                    path: path.to_path_buf(),
                    reason: format!("failed to create directory: {error}"),
                }
            })?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|error| {
            GatewayError::LoopbackRegistryWrite {
                path: path.to_path_buf(),
                reason: format!("failed to serialize: {error}"),
            }
        })?;

        let temp_path = atomic_write::temp_path(path, "loopback");
        std::fs::write(&temp_path, content).map_err(|error| {
            GatewayError::LoopbackRegistryWrite {
                path: temp_path.clone(),
                reason: error.to_string(),
            }
        })?;

        std::fs::rename(&temp_path, path).map_err(|error| GatewayError::LoopbackRegistryWrite {
            path: path.to_path_buf(),
            reason: format!("atomic rename failed: {error}"),
        })?;

        Ok(())
    }

    /// Get the assignment for an identity.
    pub fn get(&self, identity: &str) -> Option<&LoopbackAssignment> {
        self.assignments.get(identity)
    }

    /// Allocate a stable loopback IP for an identity.
    ///
    /// If the identity already has an assignment, returns it unchanged.
    pub fn allocate(
        &mut self,
        identity: &str,
        scope: &str,
    ) -> Result<&LoopbackAssignment, GatewayError> {
        self.allocate_avoiding(identity, scope, &HashSet::new())
    }

    /// Allocate a stable loopback IP for an identity while avoiding reserved
    /// IPs that are still active outside the registry snapshot.
    pub fn allocate_avoiding(
        &mut self,
        identity: &str,
        scope: &str,
        reserved: &HashSet<Ipv4Addr>,
    ) -> Result<&LoopbackAssignment, GatewayError> {
        if self.assignments.contains_key(identity) {
            return Ok(&self.assignments[identity]);
        }

        let ip = self.next_available_ip_avoiding(reserved).ok_or(
            GatewayError::LoopbackPoolExhausted {
                range_start: DEFAULT_LOOPBACK_START,
                range_end: DEFAULT_LOOPBACK_END,
            },
        )?;
        let assignment = LoopbackAssignment {
            ip,
            scope: scope.to_owned(),
        };
        self.assignments.insert(identity.to_owned(), assignment);
        Ok(&self.assignments[identity])
    }

    /// Remove an identity assignment.
    pub fn deallocate(&mut self, identity: &str) -> Option<LoopbackAssignment> {
        self.assignments.remove(identity)
    }

    /// Number of registered assignments.
    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    fn next_available_ip_avoiding(&self, reserved: &HashSet<Ipv4Addr>) -> Option<Ipv4Addr> {
        let assigned: std::collections::HashSet<Ipv4Addr> =
            self.assignments.values().map(|entry| entry.ip).collect();
        let [a, b, c, _] = DEFAULT_LOOPBACK_START.octets();
        let start = DEFAULT_LOOPBACK_START.octets()[3];
        let end = DEFAULT_LOOPBACK_END.octets()[3];
        (start..=end)
            .map(|octet| Ipv4Addr::new(a, b, c, octet))
            .find(|candidate| !assigned.contains(candidate) && !reserved.contains(candidate))
    }
}

#[cfg(test)]
#[path = "loopback/tests.rs"]
mod tests;
