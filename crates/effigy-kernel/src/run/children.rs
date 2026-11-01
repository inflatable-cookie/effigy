//! Child admission is an intent of the parent; recovery uses the same settle.
use super::*;
use crate::outcome::{CleanupOutcome, RequiredObligation};
use rusqlite::OptionalExtension;

pub(super) const KIND: &str = "kernel:child-run";

#[derive(Serialize, Deserialize)]
struct Child {
    id: RunId,
    owner: ResourceId,
}

impl<'a> Run<'a> {
    /// Admit an independently gated child. The parent intent precedes admission;
    /// the child's parent link commits with Admitted, before its first effect.
    /// Engines reject ancestor cycles before calling this method (K5).
    pub fn begin_child(
        &mut self,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Run<'a>, RunError> {
        if invocation.spec().owner == self.owner.resource {
            return Err(RunError::broken(
                "child cannot reacquire its parent's owner gate",
            ));
        }
        // Check reuse before the intent: an old id must never become a child
        // effect capable of settling another run.
        if self
            .store
            .conn()
            .query_row(
                "SELECT 1 FROM runs WHERE run_id=?1",
                [opts.id.to_string()],
                |_| Ok(()),
            )
            .optional()?
            .is_some()
        {
            return Err(RunError::refused("child run id already exists"));
        }
        let child = Child {
            id: opts.id,
            owner: invocation.spec().owner.clone(),
        };
        let ticket = self.intend_inner(EffectSpec {
            target: self.owner.clone(),
            kind: KIND.into(),
            payload: serde_json::to_value(child)?,
            settlement: Settlement::Release(Value::Null),
            required: true,
        })?;
        let detached = opts
            .custody
            .as_ref()
            .is_some_and(|plan| plan.placement == crate::supervise::Placement::Detached);
        let mut run = Self::begin_under(
            &self.cx,
            self.root,
            invocation,
            opts,
            || Ok(cleanup.clone()),
            adapter,
            Some(self.id),
            None,
            None,
            None,
            None,
        )?;
        if !detached && !run.root.is_machine() && run.ancestor_closure.is_none() {
            run.ancestor_closure = self.checkout_closure_pin()?;
        }
        if let Err(error) = self.applied(&ticket, Value::Null) {
            run.settle_inner(cleanup, adapter)?;
            return Err(error);
        }
        Ok(run)
    }

    pub(super) fn child_obligation(&self, effect: &Effect) -> RequiredObligation {
        let succeeded =
            (|| -> Result<bool, RunError> {
                let child: Child = serde_json::from_value(effect.spec.payload.clone())?;
                let record = journal::run(self.store.conn(), child.id)?;
                Ok(record.phase == Phase::Closed
                    && record.workload == Some(WorkloadOutcome::Succeeded))
            })()
            .unwrap_or(false);
        if succeeded {
            RequiredObligation::Applied
        } else {
            RequiredObligation::NotCompleted
        }
    }

    pub(super) fn settle_child(
        &mut self,
        effect: &Effect,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<(), RunError> {
        let child: Child = serde_json::from_value(effect.spec.payload.clone())?;
        let parent = self
            .store
            .conn()
            .query_row(
                "SELECT parent_run FROM runs WHERE run_id=?1",
                [child.id.to_string()],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?;
        let result = match parent {
            None => Ok(true), // intent committed, admission never happened
            Some(Some(parent)) if parent == self.id.to_string() => {
                let wait = Wait::Until(
                    std::time::Instant::now() + cleanup.operation().remaining().unwrap_or_default(),
                );
                settle::settle_interrupted_wait(
                    &settle::cleanup_effects(cleanup)?,
                    self.root,
                    (&child.owner, child.id),
                    self.next.clone(),
                    cleanup,
                    adapter,
                    wait,
                )
                .map(|outcome| outcome.cleanup() == CleanupOutcome::Complete)
            }
            _ => Err(RunError::broken("child parent mismatch")),
        };
        let (state, reason) = match result {
            Ok(true) => (EffectState::Released, None),
            Ok(false) => (
                EffectState::Residue,
                Some("child cleanup incomplete".to_owned()),
            ),
            Err(error) => (EffectState::Residue, Some(error.to_string())),
        };
        self.store.transaction_within(
            &settle::cleanup_effects(cleanup)?,
            BusyWait::AtLeast(SETTLE_BUSY_WAIT),
            |txn| {
                journal::write_effect(
                    txn,
                    self.id,
                    effect.sequence,
                    state,
                    None,
                    reason.as_deref(),
                )
            },
        )
    }
}
