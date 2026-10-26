//! One gate spelling per checkout: the token a journal recorded and the
//! path-derived identity both take the same lock, so a foreground and a durable
//! run of the same task exclude each other. Recovery takes the same spelling as
//! admission, without a central lookup.
use super::*;
use crate::id::Scope;

pub(super) fn gates(
    root: &StateRoot,
    owner: &ResourceId,
    ids: &[ResourceId],
) -> Result<Vec<ResourceId>, RunError> {
    let mut gates = ids.to_vec();
    // A machine or primitive root has one scope: there is nothing to re-spell.
    if root.is_machine() || !root.is_checkout() {
        return Ok(gates);
    }
    let checkout = root.checkout_gate_identity();
    for id in ids {
        let Scope::Checkout(scope) = id.scope() else {
            continue;
        };
        // A gate outside this checkout's own scope is not this run's to
        // re-spell; the caller's own scope always is.
        if id.scope() != owner.scope() && scope != checkout {
            continue;
        }
        gates.push(id.with_checkout(checkout).map_err(StoreError::from)?);
    }
    gates.sort_by_key(ResourceId::canonical);
    gates.dedup();
    Ok(gates)
}
