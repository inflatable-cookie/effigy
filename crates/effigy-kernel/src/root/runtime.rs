//! Production routing: one checkout journal per location, no global journal.
//! No checkout or machine generation is recorded: the journal opens by its
//! location, and gates and machine locks are named from paths (Q-011).
use super::*;
use crate::ctx::{EffectContext, ObserveContext};
use crate::location::CheckoutLocation;
use crate::store::StoreError;
use std::path::Path;
use std::time::Duration;

impl StateRoot {
    /// Pure checkout routing. The journal opens by its location (effigy#337).
    pub fn checkout_observe(cx: &ObserveContext, location: &CheckoutLocation) -> Observed<Self> {
        match Self::observe(cx, &location.root) {
            Observed::Verified(mut root, _) => {
                root.checkout = true;
                Observed::Verified(
                    root,
                    Evidence::new("verified checkout journal location".into(), None)
                        .expect("fixed source"),
                )
            }
            other => other,
        }
    }

    /// Opens one checkout journal and routes its machine facts. The journal is
    /// its location: nothing is bound, rewritten or compared first.
    pub fn checkout_effects(
        cx: &EffectContext,
        machine_home: &Path,
        location: &CheckoutLocation,
    ) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        let observe = observation(cx)?;
        let mut root = Self::effects(cx, &location.root)?;
        root.checkout = true;
        Ok(root.route_machine_gates(&observe, machine_home))
    }

    /// Pure production observation of the shared machine facts.
    pub fn runtime_machine_observe(cx: &ObserveContext, home: &Path) -> Observed<Self> {
        Self::machine_observe(cx, home)
    }

    /// Opens only shared machine facts. A legacy `kernel.db` beside them is
    /// ignored, never opened, imported or deleted.
    pub fn runtime_machine_effects(cx: &EffectContext, home: &Path) -> Result<Self, StoreError> {
        cx.require(Capability::HostAuthority)
            .map_err(|_| StoreError::HostAuthorityRequired)?;
        Self::machine_effects(cx, home).map_err(StoreError::from)
    }
}

fn observation(cx: &EffectContext) -> Result<ObserveContext, StoreError> {
    cx.child(Duration::from_secs(10))
        .operation()
        .observe()
        .map_err(|_| StoreError::Unbounded)
}
