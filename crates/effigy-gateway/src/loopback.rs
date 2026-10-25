//! Loopback alias allocation for gateway-owned route groups.
//!
//! One alias IP names one environment's services in the bounded
//! `127.1.0.x` pool. It is allocated when the environment comes up, released
//! when it goes down, and reclaimed on the next allocation when the checkout
//! path it recorded no longer exists.

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::GatewayError;

/// Build the stable registry identity for one isolated project checkout.
pub fn project_loopback_identity(project_name: &str, checkout: &Path) -> String {
    format!("project:{project_name}:{}", checkout.display())
}

/// Build the stable registry identity for one shared service checkout.
pub fn shared_loopback_identity(project_name: &str, checkout: &Path) -> String {
    format!("shared:{project_name}:{}", checkout.display())
}

/// First assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_START: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 1);

/// Last assignable loopback IP in the bounded pool.
pub const DEFAULT_LOOPBACK_END: Ipv4Addr = Ipv4Addr::new(127, 1, 0, 50);

/// One persisted loopback-IP assignment.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopbackAssignment {
    /// Stable IP reserved for the identity.
    pub ip: Ipv4Addr,

    /// Checkout path that owns the assignment.
    pub scope: String,
}

/// Loopback-IP assignments keyed by project or shared-service identity.
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

    /// Load the registry from the gateway endpoint store.
    pub fn load(path: &Path) -> Result<Self, GatewayError> {
        crate::endpoint_store::EndpointStore::for_routes(&path.with_file_name("routes.json"))
            .snapshot()?
            .loopback()
    }

    /// Get the assignment for an identity.
    pub fn get(&self, identity: &str) -> Option<&LoopbackAssignment> {
        self.assignments.get(identity)
    }

    /// Allocate the lowest free loopback IP for an identity.
    ///
    /// If the identity already has an assignment, returns it unchanged.
    pub fn allocate(
        &mut self,
        identity: &str,
        scope: &str,
    ) -> Result<&LoopbackAssignment, GatewayError> {
        if self.assignments.contains_key(identity) {
            return Ok(&self.assignments[identity]);
        }

        let ip = self
            .next_available_ip()
            .ok_or(GatewayError::LoopbackPoolExhausted {
                range_start: DEFAULT_LOOPBACK_START,
                range_end: DEFAULT_LOOPBACK_END,
            })?;
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

    /// Remove every assignment owned by one checkout path.
    pub fn release_scope(&mut self, scope: &str) -> usize {
        let before = self.assignments.len();
        self.assignments
            .retain(|_, assignment| assignment.scope != scope);
        before - self.assignments.len()
    }

    /// Drop assignments whose checkout path is gone, so a deleted worktree's
    /// address is available to the next allocation. A path that is still
    /// there keeps its assignment, whether or not it is running.
    pub fn reclaim_absent_paths(&mut self) {
        self.assignments
            .retain(|_, assignment| Path::new(&assignment.scope).exists());
    }

    /// Number of registered assignments.
    pub fn len(&self) -> usize {
        self.assignments.len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty()
    }

    fn next_available_ip(&self) -> Option<Ipv4Addr> {
        let assigned: std::collections::HashSet<Ipv4Addr> =
            self.assignments.values().map(|entry| entry.ip).collect();
        let [a, b, c, _] = DEFAULT_LOOPBACK_START.octets();
        (DEFAULT_LOOPBACK_START.octets()[3]..=DEFAULT_LOOPBACK_END.octets()[3])
            .map(|octet| Ipv4Addr::new(a, b, c, octet))
            .find(|candidate| !assigned.contains(candidate))
    }
}

#[cfg(test)]
#[path = "loopback/tests.rs"]
mod tests;
