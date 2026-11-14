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
            &mut custody.lease.guard,
            crate::supervise::memory::MemoryStart {
                executable: &sentinel.executable,
                ancestors: &owner_gates,
                cx: &self.cx,
                root: self.root,
                run: self.id,
                fence: self.owner.clone(),
                purpose: MemoryPurpose::CheckoutLease,
            },
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
    #[cfg(feature = "workspace-private-qualification")]
    pub(crate) fn qualify_workspace_discard_images(
        &mut self,
    ) -> Result<&'static crate::supervise::image_identity::CreatorImage, RunError> {
        self.can_effect()?;
        self.require_environment_gate()?;
        self.inspect_workspace_lifecycle()?;
        self.verify_custody()?;
        self.cx.operation().check("workspace helper images")?;
        let image = crate::supervise::image_identity::creator()?;
        let sentinel = self
            .sentinel
            .as_ref()
            .ok_or_else(|| RunError::refused("workspace sentinel missing"))?;
        image.verify_process(&sentinel.identity)?;
        let backup = self
            .checkout_custody
            .as_mut()
            .and_then(|custody| custody.backup.as_mut())
            .ok_or_else(|| RunError::refused("workspace checkout backup missing"))?;
        backup.verify_retained()?;
        let identity = backup
            .identity
            .as_ref()
            .ok_or_else(|| RunError::refused("workspace backup identity missing"))?;
        image.verify_process(identity)?;
        self.cx.operation().check("workspace helper images")?;
        Ok(image)
    }

    pub(crate) fn hold_prelaunch_interpolation(
        &mut self,
        slot: &std::sync::Arc<std::sync::Mutex<crate::interpolation::PrelaunchInterpolation>>,
    ) -> Result<(), RunError> {
        let mut slot = slot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let sequence = slot
            .sequence
            .ok_or_else(|| RunError::unavailable("prelaunch sequence unavailable"))?;
        slot.hold_attempted = true;
        let backup = self
            .checkout_custody
            .as_mut()
            .and_then(|custody| custody.backup.as_mut())
            .ok_or_else(|| RunError::unavailable("prelaunch checkout custodian unavailable"))?;
        backup.retain_prelaunch(
            sequence,
            slot.file
                .as_ref()
                .ok_or_else(|| RunError::unavailable("prelaunch file unavailable"))?,
        )
    }
    pub(super) fn release_prelaunch_interpolation(
        &mut self,
        sequence: i64,
    ) -> Result<(), RunError> {
        self.checkout_custody
            .as_mut()
            .and_then(|custody| custody.backup.as_mut())
            .ok_or_else(|| RunError::unavailable("prelaunch custodian unavailable"))?
            .release_prelaunch(sequence)
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
                crate::workspace_session::check_run_fence(txn.conn(), self.id, &self.owner)?;
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
                crate::workspace_session::check_run_fence(txn.conn(), self.id, &self.owner)?;
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
        self.close_checkout_backup_recoverable(cleanup)?;
        self.checkout_custody.take();
        self.ancestor_closure.take();
        Ok(())
    }

    fn close_checkout_backup_recoverable(
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
        if let Some(custody) = self.checkout_custody.as_mut() {
            custody.backup.take();
        }
        Ok(())
    }

    /// Parking may close shared Env descriptions only after every private
    /// workload and its exact backup have positively closed. The checkout
    /// lease and sentinel stay on this same Run across gate-free integrations.
    #[cfg(feature = "workspace-private-qualification")]
    pub(super) fn prepare_qualification_park(
        &mut self,
        deadline: std::time::Instant,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        let cx = self.cx.child_until(deadline);
        cx.operation().check("workspace private park")?;
        let cleanup = cx.operation().cleanup_within();
        let effects = self.effects()?;
        for effect in effects
            .iter()
            .filter(|effect| effect.spec.kind == crate::custody::KIND && !effect.state.terminal())
        {
            if !matches!(
                self.completed.get(&effect.sequence),
                Some(crate::outcome::RequiredObligation::ProcessExit(_))
            ) {
                return Err(RunError::unavailable(
                    "workspace private workload completion unavailable",
                ));
            }
            let core = self.processes.get(&effect.sequence).ok_or_else(|| {
                RunError::unavailable("workspace private workload owner unavailable")
            })?;
            crate::custody::verify_qualification_closure(core, cx.operation())?;
        }
        for effect in effects
            .iter()
            .rev()
            .filter(|effect| effect.spec.kind == crate::custody::KIND && !effect.state.terminal())
        {
            let (state, _) = crate::custody::settle_effect(
                effect,
                self.processes.get(&effect.sequence).cloned(),
                &cleanup,
                || false,
            );
            if state != crate::run::EffectState::Released {
                return Err(RunError::unavailable(
                    "workspace private workload closure remains Unknown",
                ));
            }
            // Keep Applied until final settlement: publication's required-effect
            // predicate still needs the successful apply. The exact live core
            // retains its verified exit/closure and idempotent stop state.
        }
        self.close_checkout_backup_recoverable(&cleanup)?;
        cx.operation()
            .check("workspace private custody closed for park")?;
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
