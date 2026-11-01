//! Shared checkout lifetime remains protected through native process cleanup,
//! even after the checkout and its journal are externally deleted.
use super::*;
use crate::closure::ClosurePin;
use crate::gate::{CheckoutFenceGuard, CheckoutFenceIdentity};
use crate::supervise::memory::{MemoryGuard, MemoryPurpose, PartialMemoryGuard};

#[derive(Debug)]
pub(super) struct CheckoutCustody {
    lease: CheckoutFenceGuard,
    backup: Option<MemoryGuard>,
    partial_backup: Option<Box<PartialMemoryGuard>>,
    token: String,
}
impl Run<'_> {
    pub(crate) fn protect_checkout(
        &mut self,
        checkout: crate::id::CheckoutId,
        wait: Wait,
    ) -> Result<(), RunError> {
        if self.checkout_custody.is_some() || self.root.is_machine() {
            return Ok(());
        }
        let token = crate::store::token(&checkout);
        let lease = Gate::checkout_custody(&self.cx, self.root, &token, wait)?;
        self.checkout_custody = Some(CheckoutCustody {
            lease,
            backup: None,
            partial_backup: None,
            token,
        });
        self.start_checkout_backup()
    }
    pub(super) fn start_checkout_backup(&mut self) -> Result<(), RunError> {
        let Some(custody) = &mut self.checkout_custody else {
            return Ok(());
        };
        if custody.partial_backup.is_some() {
            return Err(RunError::unavailable(
                "checkout backup startup remains unsettled",
            ));
        }
        if custody.backup.is_some() {
            return Ok(());
        }
        let Some(sentinel) = &self.sentinel else {
            return Ok(());
        };
        match MemoryGuard::start_recoverable(
            &sentinel.executable,
            &mut custody.lease.guard,
            &[],
            &self.cx,
            self.id,
            self.owner.clone(),
            MemoryPurpose::CheckoutLease,
        ) {
            Ok(backup) => {
                custody.backup = Some(backup);
                Ok(())
            }
            Err((error, partial)) => {
                custody.partial_backup = partial;
                Err(error)
            }
        }
    }
    pub(crate) fn acknowledge_checkout_process(
        &mut self,
        ticket: &EffectTicket,
    ) -> Result<(), RunError> {
        if self
            .checkout_custody
            .as_ref()
            .is_some_and(|custody| custody.partial_backup.is_some())
        {
            return Err(RunError::unavailable(
                "checkout backup startup remains unsettled",
            ));
        }
        if let Some(backup) = self
            .checkout_custody
            .as_mut()
            .and_then(|c| c.backup.as_mut())
        {
            let effect = journal::effect(self.store.conn(), self.id, ticket.sequence)?;
            backup.register(&effect)?;
        }
        Ok(())
    }
    pub(crate) fn checkout_closure_pin(&self) -> Result<Option<ClosurePin>, RunError> {
        match self
            .checkout_custody
            .as_ref()
            .and_then(|c| c.backup.as_ref())
        {
            Some(backup) => backup.closure_pin().map(Some),
            None => Ok(self.ancestor_closure.clone()),
        }
    }
    /// Current inode stamp for the environment's external ownership labels.
    pub fn checkout_fence_identity(&self) -> Option<CheckoutFenceIdentity> {
        self.checkout_custody.as_ref().map(|c| c.lease.identity())
    }
    pub(super) fn verify_checkout_custody(&self) -> Result<(), RunError> {
        if let Some(custody) = &self.checkout_custody {
            if custody.partial_backup.is_some() {
                return Err(RunError::unavailable(
                    "checkout backup startup remains unsettled",
                ));
            }
            let cx = self
                .cx
                .child(std::time::Duration::from_secs(3))
                .operation()
                .observe()
                .map_err(|_| RunError::unavailable("checkout fence observation context"))?;
            match Gate::checkout_fence_identity(&cx, self.root, &custody.token) {
                crate::observe::Observed::Verified(actual, _)
                    if actual == custody.lease.identity() => {}
                other => {
                    return Err(RunError::refused(format!(
                        "the checkout custody fence changed or is unknown ({other:?})"
                    )))
                }
            }
        }
        Ok(())
    }
    pub(super) fn close_checkout_custody(
        &mut self,
        cleanup: &CleanupContext,
    ) -> Result<(), RunError> {
        self.close_checkout_custody_recoverable(cleanup)
    }

    /// Keep the same backup lease available if bounded cleanup fails.
    pub(super) fn close_checkout_custody_recoverable(
        &mut self,
        cleanup: &CleanupContext,
    ) -> Result<(), RunError> {
        if let Some(custody) = self.checkout_custody.as_mut() {
            if let Some(backup) = custody.partial_backup.as_mut() {
                backup.close(cleanup)?;
            }
            custody.partial_backup.take();
            if let Some(backup) = custody.backup.as_mut() {
                backup.close(cleanup)?;
            }
        }
        self.checkout_custody.take();
        self.ancestor_closure.take();
        Ok(())
    }

    #[cfg(all(test, feature = "fault-injection"))]
    pub(super) fn has_partial_checkout_backup(&self) -> bool {
        self.checkout_custody
            .as_ref()
            .is_some_and(|custody| custody.partial_backup.is_some())
    }

    #[cfg(all(test, feature = "fault-injection"))]
    pub(super) fn partial_checkout_backup_identity(&self) -> Option<crate::ident::ProcessIdentity> {
        self.checkout_custody
            .as_ref()?
            .partial_backup
            .as_ref()?
            .identity()
    }
}
