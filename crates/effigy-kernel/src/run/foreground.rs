//! Checkout-local exclusion and memory-only custody; no writable journal.
use super::*;
use crate::ctx::{EffectClass, Placement};
use crate::id::{ExecutionTarget, ResourceKind, Scope};
use crate::ident::ProcessIdentity;
use crate::observe::Observed;
use crate::supervise::{memory::MemoryGuard, Executable};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A foreground run has no persisted incarnation, requests, logs or history.
/// Its process receipt reaches both custodians before workload go. The owner
/// and memory sentinel hold the same gate descriptions through cleanup.
#[derive(Debug)]
pub struct ForegroundRun {
    record: ForegroundRecord,
    cx: EffectContext,
    next: Invocation,
    executable: Executable,
    guard: Option<MemoryGuard>,
    processes: BTreeMap<i64, Arc<Mutex<crate::custody::ProcessCore>>>,
    _gate: GateGuard,
}
impl ForegroundRun {
    /// The caller classifies the whole tree before any effect, and supplies
    /// identities resolved under this checkout. An unresolved predecessor
    /// requires a separate durable recovery; this path only reads its journal.
    pub fn begin(
        cx: &EffectContext,
        root: &StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
    ) -> Result<Self, RunError> {
        if cx.operation().placement() != Placement::Host || inv.spec().placement != Placement::Host
        {
            return Err(RunError::HostAuthorityRequired);
        }
        if cx.operation().class() != EffectClass::Mutate
            || inv.spec().class != EffectClass::Mutate
            || !matches!(inv.spec().target, ExecutionTarget::Host)
            || !inv.spec().attachments.is_empty()
            || root.database_name() != crate::store::DB_NAME
            || !matches!(inv.spec().owner.scope(), Scope::Checkout(_))
            || !matches!(
                inv.spec().owner.kind(),
                ResourceKind::Task | ResourceKind::Store
            )
            || !opts.acknowledge.is_empty()
        {
            return Err(RunError::Invalid(
                "foreground admission requires local checkout capabilities".into(),
            ));
        }
        cx.operation().check("foreground admission")?;
        let publication = cx
            .child(Duration::from_secs(10))
            .operation()
            .observe()
            .map_err(|_| RunError::Invalid("foreground publication context".into()))?;
        crate::migration::adoption::check_invocation(&publication, root, inv)?;
        let plan = opts
            .custody
            .ok_or_else(|| RunError::Invalid("foreground custody needs a memory guard".into()))?;
        if plan.placement != crate::supervise::Placement::Inline {
            return Err(RunError::Invalid("foreground custody cannot detach".into()));
        }
        let mut ids = inv.spec().gates.clone();
        ids.push(inv.spec().owner.clone());
        if ids.iter().any(|id| {
            (id.scope() != inv.spec().owner.scope() && !id.checkout_alias_of(&inv.spec().owner))
                || !matches!(id.scope(), Scope::Checkout(_))
                || !matches!(id.kind(), ResourceKind::Task | ResourceKind::Store)
        }) {
            return Err(RunError::Invalid(
                "foreground gates must belong to the owning checkout".into(),
            ));
        }
        let mut gate = Gate::acquire(cx, root, &ids, opts.wait)?;
        // Declared shared exclusion gates remain held too; adapter gates
        // cannot reach this local-only path.
        let observe = cx
            .child(Duration::from_secs(10))
            .operation()
            .observe()
            .map_err(|_| RunError::Invalid("foreground predecessor read context".into()))?;
        match Store::read(&observe, root) {
            Observed::Verified(reader, _) => {
                for owner in &ids {
                    for previous in journal::predecessors(reader.conn(), owner)? {
                        if !previous.acknowledged {
                            return Err(RunError::PredecessorUnsettled {
                                run: previous.id,
                                effects: journal::effects(reader.conn(), previous.id)?,
                            });
                        }
                    }
                }
            }
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => {
                return Err(RunError::Invalid(format!(
                    "foreground predecessor evidence unavailable: {reason:?}"
                )))
            }
        }
        gate.set_run(&opts.id);
        let guard = MemoryGuard::start(
            &plan.executable,
            &mut gate,
            cx,
            opts.id,
            Fence {
                resource: inv.spec().owner.clone(),
                generation: Generation::new(0),
            },
        )?;
        Ok(Self {
            record: ForegroundRecord::new(opts.id, inv.spec().owner.clone()),
            cx: cx.clone(),
            next: opts.next,
            executable: plan.executable,
            guard: Some(guard),
            processes: BTreeMap::new(),
            _gate: gate,
        })
    }
    pub fn record(&self) -> &ForegroundRecord {
        &self.record
    }
    pub(crate) fn record_mut(&mut self) -> &mut ForegroundRecord {
        &mut self.record
    }
    pub fn guard_identity(&self) -> Option<&ProcessIdentity> {
        self.guard.as_ref().and_then(|g| g.identity.as_ref())
    }
    pub(crate) fn has_guard(&self) -> bool {
        self.guard.is_some()
    }
    pub(crate) fn anchor_image(&self) -> Result<crate::supervise::anchor::Image<'_>, RunError> {
        crate::supervise::anchor::Image::memory(&self.executable)
    }
    pub(crate) fn ack_process(&mut self, ticket: &EffectTicket) -> Result<(), RunError> {
        let effect = self
            .record
            .effects()
            .iter()
            .find(|e| e.sequence == ticket.sequence)
            .ok_or_else(|| RunError::Invalid("missing foreground process receipt".into()))?;
        self.guard
            .as_mut()
            .ok_or_else(|| RunError::Invalid("foreground guard unavailable".into()))?
            .register(effect)
    }
    pub(crate) fn retain_process(
        &mut self,
        sequence: i64,
        core: Arc<Mutex<crate::custody::ProcessCore>>,
    ) {
        self.processes.insert(sequence, core);
    }
    pub fn advance(&mut self, phase: Phase) -> Result<(), RunError> {
        self.cx.operation().check("foreground phase")?;
        self.record.advance(phase)
    }
    pub fn finish(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        self.cx.operation().check("foreground finish")?;
        self.record.finish(workload)
    }
    pub fn interrupt(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        self.record.interrupt(workload)
    }
    pub fn settle(mut self, cleanup: &CleanupContext) -> Result<Outcome<Settled>, RunError> {
        for effect in self.record.effects().to_vec().iter().rev() {
            if effect.state.terminal() {
                continue;
            }
            cleanup.operation().check("foreground settlement")?;
            let live = self.processes.remove(&effect.sequence);
            let (state, reason) =
                crate::custody::settle_effect(effect, live, cleanup, crate::signal::escalated);
            self.record.settle_effect(effect.sequence, state, reason)?;
        }
        self.record.close()?;
        if let Some(mut guard) = self.guard.take() {
            guard.close(cleanup)?;
        }
        settle::outcome(
            self.record.incarnation().run,
            &self.next,
            self.record
                .workload()
                .cloned()
                .unwrap_or(WorkloadOutcome::Interrupted),
            self.record.effects().to_vec(),
        )
    }
}
impl Drop for ForegroundRun {
    fn drop(&mut self) {
        // Owner custody remains if the guard died. A guard retains the locks
        // if the owner cannot finish; neither case permits numeric replay.
        if let Ok(cleanup) = self.cx.operation().cleanup(Duration::from_secs(60)) {
            for core in self.processes.values().rev() {
                core.lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .stop(&cleanup);
            }
            if let Some(mut guard) = self.guard.take() {
                let _ = guard.close(&cleanup);
            }
        }
    }
}
