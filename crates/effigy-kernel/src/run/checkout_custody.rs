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
        // Share the Run's actual exclusion descriptions as well as the
        // checkout lease. A pathname reopened by the helper would be a different
        // flock owner; explicit unlock on caller drop would release both copies.
        let owner_gates = self.guard.share_custody()?;
        match MemoryGuard::start_recoverable(
            &sentinel.executable,
            &mut custody.lease.guard,
            &owner_gates,
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
    pub(crate) fn retain_checkout_interpolation(
        &mut self,
        ticket: &EffectTicket,
        file: &mut crate::interpolation::RetainedInterpolation,
    ) -> Result<(), RunError> {
        let effect = journal::effect(self.store.conn(), self.id, ticket.sequence)?;
        let backup = self
            .checkout_custody
            .as_mut()
            .and_then(|c| c.backup.as_mut())
            .ok_or_else(|| RunError::refused("interpolation requires retained checkout backup"))?;
        backup.verify_retained()?;
        let process_receipt = effect
            .receipt
            .clone()
            .ok_or_else(|| RunError::broken("missing interpolation process receipt"))?;
        if process_receipt.get("interpolation_v1").is_none() {
            let pending = serde_json::json!({"interpolation_v1": process_receipt, "binding": {"pending": true}});
            let publication = self.store.transaction(&self.cx, |txn| {
                journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                journal::write_effect(
                    txn,
                    self.id,
                    ticket.sequence,
                    EffectState::Applied,
                    Some(&pending),
                    None,
                )
            });
            if publication.is_err() {
                self.poisoned = true;
            }
            publication?;
        }
        if file.binding().is_ok() {
            if !file.matches_effect(&effect, self.id, &self.owner) {
                return Err(RunError::refused(
                    "retained interpolation retry binding differs",
                ));
            }
        } else {
            file.bind(self.id, self.owner.clone(), effect.clone())?;
        }
        // The Run journal records the obligation before the helper handoff or
        // go. The distinct envelope is unreadable to old process-receipt
        // readers: owner loss cannot silently turn a bound file into a plain
        // process effect whose release would claim complete cleanup.
        let mut receipt = effect
            .receipt
            .clone()
            .ok_or_else(|| RunError::broken("missing interpolation process receipt"))?;
        if receipt.get("interpolation_v1").is_none() {
            receipt = serde_json::json!({"interpolation_v1": receipt, "binding": file.binding()?});
            let publication = self.store.transaction(&self.cx, |txn| {
                journal::check_run_fence(txn.conn(), self.id, &self.owner)?;
                journal::write_effect(
                    txn,
                    self.id,
                    ticket.sequence,
                    EffectState::Applied,
                    Some(&receipt),
                    None,
                )
            });
            if publication.is_err() {
                self.poisoned = true;
            }
            publication?;
        }
        backup.retain_interpolation(file)
    }
    pub(super) fn finish_interpolation_custody(
        &mut self,
        cleanup: &CleanupContext,
    ) -> Result<(), RunError> {
        let pending = self.processes.values().any(|core| {
            core.lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .interpolation
                .as_ref()
                .is_some_and(|file| !file.helper_completed())
        });
        if !pending {
            return Ok(());
        }
        let backup = self
            .checkout_custody
            .as_mut()
            .and_then(|custody| custody.backup.as_mut())
            .ok_or_else(|| {
                RunError::unavailable("interpolation has no verified completion custodian")
            })?;
        if backup.has_unconfirmed_files() {
            for core in self.processes.values() {
                let core = core
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if let Some(file) = core.interpolation.as_ref() {
                    backup.retain_interpolation(file)?;
                }
            }
        }
        for core in self.processes.values() {
            let core = core
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if core
                .interpolation
                .as_ref()
                .is_some_and(|file| !file.helper_completed() && !backup.owns_interpolation(file))
            {
                return Err(RunError::unavailable(
                    "interpolation descriptor handoff did not complete",
                ));
            }
        }
        self.close_checkout_custody_recoverable(cleanup)
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
                backup.close(cleanup).map_err(|_| {
                    RunError::unavailable("checkout backup cleanup remains unsettled")
                })?;
            }
            custody.partial_backup.take();
            if let Some(backup) = custody.backup.as_mut() {
                backup.close(cleanup)?;
            }
        }
        if self
            .checkout_custody
            .as_ref()
            .and_then(|custody| custody.backup.as_ref())
            .is_some_and(|backup| backup.has_interpolation())
        {
            for core in self.processes.values() {
                if let Some(file) = core
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .interpolation
                    .as_mut()
                {
                    if self
                        .checkout_custody
                        .as_ref()
                        .and_then(|custody| custody.backup.as_ref())
                        .is_some_and(|backup| backup.owns_interpolation(file))
                    {
                        file.confirm_helper_completion();
                    }
                }
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

    /// The exact qualified backup this run retained, for tests that must stop
    /// or reobserve that one incarnation and no other process.
    #[cfg(all(test, feature = "fault-injection"))]
    pub(super) fn checkout_backup_identity(&self) -> Option<crate::ident::ProcessIdentity> {
        self.checkout_custody
            .as_ref()?
            .backup
            .as_ref()?
            .identity
            .clone()
    }
}
