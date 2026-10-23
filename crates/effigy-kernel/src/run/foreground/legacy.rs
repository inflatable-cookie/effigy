//! One-time exclusion against observed pre-relocation owners. No writable
//! journal handle escapes this capability; only native gate custody is shared.
use super::*;
use crate::ctx::ObserveContext;
use crate::id::{CheckoutId, CheckoutKind};
use std::path::Path;

#[derive(Debug)]
pub struct LegacyForeground {
    pub(super) root: StateRoot,
    owners: Vec<ResourceId>,
}

impl LegacyForeground {
    /// Persisted aliases come from this checkout's verified token files.
    /// Token-free owners have no old spelling; published checkout authority
    /// also bypasses the machine source entirely.
    pub fn for_checkout(
        cx: &ObserveContext,
        local: &StateRoot,
        machine_home: Option<&Path>,
        invocation: &ResolvedInvocation,
        aliases: &[CheckoutId],
    ) -> Result<Option<Self>, RunError> {
        if aliases.is_empty() {
            return Ok(None);
        }
        if aliases.len() > 32
            || aliases
                .iter()
                .any(|id| matches!(id.kind(), CheckoutKind::Machine | CheckoutKind::Filesystem))
        {
            return Err(RunError::Invalid("invalid legacy checkout aliases".into()));
        }
        let Some(root) = StateRoot::foreground_legacy(cx, local, machine_home)? else {
            return Ok(None);
        };
        let mut owners = Vec::new();
        for id in std::iter::once(&invocation.spec().owner).chain(&invocation.spec().gates) {
            if !matches!(id.scope(), Scope::Checkout(_))
                || !matches!(id.kind(), ResourceKind::Task | ResourceKind::Store)
            {
                return Err(RunError::Invalid(
                    "legacy foreground exclusion requires checkout resources".into(),
                ));
            }
            for alias in aliases {
                let id = id.with_checkout(*alias).map_err(StoreError::from)?;
                if !owners.contains(&id) {
                    owners.push(id);
                }
                if owners.len() > 64 {
                    return Err(RunError::Invalid(
                        "too many legacy foreground exclusion gates".into(),
                    ));
                }
            }
        }
        Ok(Some(Self { root, owners }))
    }

    pub(super) fn acquire(&self, cx: &EffectContext, wait: Wait) -> Result<GateGuard, RunError> {
        self.root.verify_binding().map_err(StoreError::from)?;
        self.root
            .state_directory()
            .require_local()
            .map_err(StoreError::from)?;
        // This private view can acquire file locks, never a writable SQLite
        // connection. The original source remains the sole legacy authority.
        let gate = Gate::acquire(cx, &self.root.legacy_control(), &self.owners, wait)?;
        check_predecessors(cx, &self.root, &self.owners).map_err(|error| RunError::Invalid(format!(
            "legacy foreground predecessor requires recovery or relocation: {error:?}; run `effigy run migrate --plan`; nothing was started"
        )))?;
        Ok(gate)
    }
}
