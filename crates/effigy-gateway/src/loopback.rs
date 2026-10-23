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

use crate::error::GatewayError;
use crate::routes::{RouteSource, RouteTable};

pub mod reclaim;

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
        crate::endpoint_store::EndpointStore::for_routes(&path.with_file_name("routes.json"))
            .read_compatible()?
            .loopback()
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

    /// Reallocate an identity whose stored IP is now reserved by another
    /// project's container route.
    ///
    /// Pruning drops registry assignments for stacks that are not running,
    /// but their container routes stay registered and a stale route's
    /// gateway TCP listener can still bind the tuple. When that happens the
    /// stored IP no longer names this identity's listener, so keeping it
    /// would republish Compose ports on an address a stale listener holds.
    /// Returns true when the identity was deallocated and needs a fresh
    /// allocation.
    pub fn deallocate_if_reserved(&mut self, identity: &str, reserved: &HashSet<Ipv4Addr>) -> bool {
        let collides = self
            .get(identity)
            .is_some_and(|assignment| reserved.contains(&assignment.ip));
        if collides {
            self.deallocate(identity);
        }
        collides
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

/// Loopback IPs held by other projects' container routes that a new
/// allocation for `project_path` must avoid.
///
/// This is the one shared reservation both allocation paths call. Compose
/// generation (`effigy-containers`) and gateway registration previously
/// disagreed here: generation only avoided IPs already present in
/// `loopback-ips.json` while
/// registration also reserved every other project's container-route `dns_ip`.
/// Pruning drops assignments for stacks that are not running, but their
/// routes stay in `routes.json`, so generation could publish Compose ports on
/// an IP a stale route's gateway TCP listener still binds. Registration then
/// reallocated, and DNS, the runtime binding and the gateway listener ended
/// up on different IPs while the mapped localhost ports stayed ready — the
/// host alias hung against a dead or wrong upstream. Reserving in both paths
/// keeps the compose-published IP and the registered route IP identical for
/// one identity.
///
/// Only `RouteSource::Container` routes participate: DNS-only or externally
/// owned routes never back a gateway TCP alias listener.
///
/// Stale-route cleanup for missing checkouts (`effigy container retire
/// --dead`) owns deleting the routes themselves; this reservation only stops
/// new allocations from colliding with them.
pub fn reserved_loopback_ips_for_other_projects(
    route_table: &RouteTable,
    project_path: &str,
) -> HashSet<Ipv4Addr> {
    route_table
        .all_routes()
        .into_iter()
        .filter(|route| route.source == RouteSource::Container)
        .filter(|route| route.project != project_path)
        .filter_map(|route| route.dns_ip)
        .collect()
}

#[cfg(test)]
#[path = "loopback/tests.rs"]
mod tests;
