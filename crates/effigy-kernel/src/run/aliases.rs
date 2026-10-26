//! A checkout's filesystem and published-token spellings share exclusion.
//! Recovery takes the same aliases as admission, without a central lookup.
use super::*;
use crate::ctx::ObserveContext;
use crate::id::{CheckoutKind, Scope};

pub(super) fn gates(
    cx: &ObserveContext,
    root: &StateRoot,
    owner: &ResourceId,
    ids: &[ResourceId],
) -> Result<Vec<ResourceId>, RunError> {
    let mut gates = ids.to_vec();
    let Some(local) = root.local_gate_identity(cx)? else {
        return Ok(gates);
    };
    let checkouts = vec![local, root.directory_gate_identity()?];
    for id in ids {
        if let Scope::Checkout(checkout) = id.scope() {
            if checkout.kind() == CheckoutKind::Filesystem && checkout != local {
                return Err(RunError::refused(
                    "filesystem gate belongs to a different checkout generation",
                ));
            }
            if id.scope() != owner.scope() && !checkouts.contains(&checkout) {
                continue;
            }
            for checkout in &checkouts {
                gates.push(id.with_checkout(*checkout).map_err(StoreError::from)?);
            }
        }
    }
    gates.sort_by_key(ResourceId::canonical);
    gates.dedup();
    Ok(gates)
}
