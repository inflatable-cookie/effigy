//! Explicit release of loopback assignments, container routes and withdrawn
//! domain claims whose owner is provably gone.
//!
//! The caller proves every entry dead outside this module; here the caller's
//! decision is recomputed under the endpoint Store's owner gate and applied in
//! one publication, preserving unknown fields. The pre-edit documents are kept
//! as a journaled backup so an operator can inspect exactly what was released.

use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use crate::endpoint_store::{DomainClaim, EndpointStore};
use crate::error::GatewayError;
use crate::loopback::{LoopbackAssignment, LoopbackRegistry};
use crate::ports::sha256_hex;
use crate::routes::{Route, RouteTable};

/// One exact set of dead endpoints to release.
///
/// Entries carry the values the caller planned, so a release refuses when the
/// Store changed between planning and writing. Withdrawn domain claims are
/// released only while they stay ownerless and unrouted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadClaimEdit {
    /// Assignments to drop, each with the planned address and lifetime owner.
    pub assignments: Vec<(String, LoopbackAssignment)>,

    /// Container routes to drop, each with the planned route.
    pub routes: Vec<Route>,

    /// Withdrawn persistent domain claims to drop, each with its planned owner.
    pub domains: Vec<DomainClaim>,
}

impl DeadClaimEdit {
    /// Whether the edit releases nothing.
    pub fn is_empty(&self) -> bool {
        self.assignments.is_empty() && self.routes.is_empty() && self.domains.is_empty()
    }

    /// Addresses the edit releases.
    pub fn released_addresses(&self) -> Vec<Ipv4Addr> {
        self.assignments
            .iter()
            .map(|(_, assignment)| assignment.ip)
            .collect()
    }
}

/// What the Store released, with the audit trail of the write.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeadClaimOutcome {
    /// Identities whose assignment was removed.
    pub released_assignments: Vec<String>,

    /// Domains whose route was removed.
    pub released_routes: Vec<String>,

    /// Withdrawn domain claims whose persistent owner was removed.
    pub released_domains: Vec<String>,

    /// Addresses released by this edit, in assignment order.
    pub released_addresses: Vec<Ipv4Addr>,

    /// Pre-edit copy of every endpoint document this edit can change, present
    /// only when something was released.
    pub backup: Option<PathBuf>,

    /// SHA-256 of the pre-edit documents, present only when something was
    /// released.
    pub sha256_before: Option<String>,

    /// SHA-256 of the post-edit documents, present only when something was
    /// released.
    pub sha256_after: Option<String>,
}

/// Recompute one release decision under the endpoint Store's owner gate, then
/// apply it in a single publication.
///
/// `decide` sees the loopback registry, route table and persistent domain
/// claims as they are under the gate, so every check it makes is revalidated
/// against the snapshot that gets written back. An edit that releases nothing
/// writes nothing and takes no backup.
pub fn edit_locked<T, E: From<GatewayError>>(
    store: &EndpointStore,
    decide: impl FnOnce(&LoopbackRegistry, &RouteTable, &[DomainClaim]) -> Result<(DeadClaimEdit, T), E>,
) -> Result<(DeadClaimOutcome, T), E> {
    store.transaction_with_backup(None, |snapshot| {
        let registry = snapshot.loopback()?;
        let routes = snapshot.routes()?;
        let claims = snapshot.domain_claims();
        let (edit, value) = decide(&registry, &routes, &claims)?;
        if edit.is_empty() {
            return Ok(((DeadClaimOutcome::default(), value), None));
        }

        let before = documents_bytes(&registry, &routes, &claims)?;
        for (identity, expected) in &edit.assignments {
            match registry.get(identity) {
                Some(current) if current == expected => {}
                Some(_) => {
                    return Err(GatewayError::DeadClaimChanged {
                        entry: identity.clone(),
                    }
                    .into())
                }
                None => {
                    return Err(GatewayError::DeadClaimChanged {
                        entry: identity.clone(),
                    }
                    .into())
                }
            }
        }
        for route in &edit.routes {
            if routes.lookup(&route.domain) != Some(route) {
                return Err(GatewayError::DeadClaimChanged {
                    entry: route.domain.clone(),
                }
                .into());
            }
        }
        for claim in &edit.domains {
            if !claims.iter().any(|current| current == claim) {
                return Err(GatewayError::DeadClaimChanged {
                    entry: claim.domain.clone(),
                }
                .into());
            }
        }

        for route in &edit.routes {
            snapshot.release_route(route)?;
        }
        for claim in &edit.domains {
            snapshot.release_domain_claim(claim)?;
        }
        let mut remaining = registry.clone();
        for (identity, _) in &edit.assignments {
            remaining.deallocate(identity);
        }
        snapshot.set_loopback(&remaining)?;

        let after = documents_bytes(&remaining, &snapshot.routes()?, &snapshot.domain_claims())?;
        let backup = backup_path(store);
        let outcome = DeadClaimOutcome {
            released_assignments: edit
                .assignments
                .iter()
                .map(|(identity, _)| identity.clone())
                .collect(),
            released_routes: edit
                .routes
                .iter()
                .map(|route| route.domain.clone())
                .collect(),
            released_domains: edit
                .domains
                .iter()
                .map(|claim| claim.domain.clone())
                .collect(),
            released_addresses: edit.released_addresses(),
            backup: Some(backup.clone()),
            sha256_before: Some(sha256_hex(&before)),
            sha256_after: Some(sha256_hex(&after)),
        };
        Ok(((outcome, value), Some((backup, before))))
    })
}

/// Every endpoint document this edit can change, serialized for the backup and
/// for SHA-256 evidence.
fn documents_bytes(
    registry: &LoopbackRegistry,
    routes: &RouteTable,
    claims: &[DomainClaim],
) -> Result<Vec<u8>, GatewayError> {
    let store_error = |error: serde_json::Error| GatewayError::EndpointStore {
        reason: error.to_string(),
    };
    let domain_claims = claims
        .iter()
        .map(|claim| {
            json!({
                "domain": claim.domain,
                "project": claim.project,
                "scope": claim.scope,
                "listener": claim.listener,
            })
        })
        .collect::<Vec<_>>();
    let document = json!({
        "loopback": serde_json::to_value(registry).map_err(store_error)?,
        "routes": serde_json::to_value(routes).map_err(store_error)?,
        "domain_claims": domain_claims,
    });
    let mut bytes = serde_json::to_vec_pretty(&document).map_err(store_error)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Legacy gateway state directory: `~/.effigy/gateway` for the standard home,
/// or `<root>/gateway` for an isolated private root.
pub fn gateway_state_directory(store: &EndpointStore) -> PathBuf {
    let standard = store.home().join(".effigy").join("gateway");
    if standard.is_dir() {
        return standard;
    }
    store.home().join("gateway")
}

fn backup_path(store: &EndpointStore) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_nanos())
        .unwrap_or_default();
    gateway_state_directory(store).join(format!("loopback-dead-reclaim-backup-{nanos}.json"))
}

#[cfg(test)]
#[path = "reclaim/tests.rs"]
mod tests;
