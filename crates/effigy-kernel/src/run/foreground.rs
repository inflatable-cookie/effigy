//! Checkout-local exclusion and memory-only custody with a terminal journal record.
use super::*;
use crate::ctx::{EffectClass, Placement};
use crate::id::{ExecutionTarget, ResourceKind, Scope};
use crate::ident::ProcessIdentity;
use crate::outcome::CleanupOutcome;
use crate::supervise::{memory::MemoryGuard, Executable};
use std::collections::BTreeMap;
use std::os::fd::OwnedFd;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A foreground run has no persisted incarnation, requests, logs or history.
/// Its process receipt reaches both custodians before workload go. The owner
/// and memory sentinel hold the same gate descriptions through cleanup.
#[derive(Debug)]
pub struct ForegroundRun {
    record: ForegroundRecord,
    cx: EffectContext,
    root: StateRoot,
    task: String,
    boot: String,
    next: Invocation,
    executable: Executable,
    guard: Option<MemoryGuard>,
    processes: BTreeMap<i64, Arc<Mutex<crate::custody::ProcessCore>>>,
    _gate: GateGuard,
    ancestor_gates: Vec<OwnedFd>,
    ancestor_owners: Vec<ResourceId>,
    children: Vec<Arc<Mutex<Option<WorkloadOutcome>>>>,
    completion: Option<Arc<Mutex<Option<WorkloadOutcome>>>>,
    /// The invocation's absolute run deadline cut settlement short: the
    /// surviving custodian is kept and no closure is claimed (effigy#992).
    truncated: bool,
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
        Self::begin_under(cx, root, inv, opts, Vec::new(), Vec::new())
    }

    /// Each child's memory guard also holds every ancestor's gates. Owner
    /// death can therefore never release a parent while a child is cleaning
    /// up, even if the parent's guard finishes first. No descriptors reach
    /// the workload. Success requires positive child completion.
    pub fn begin_child(
        &mut self,
        root: &StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
    ) -> Result<Self, RunError> {
        self.begin_child_under(root, inv, opts)
    }

    /// Ancestor descriptions stay held through child cleanup.
    fn begin_child_under(
        &mut self,
        root: &StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
    ) -> Result<Self, RunError> {
        if self.ancestor_owners.contains(&inv.spec().owner)
            || self.record.incarnation().owner == inv.spec().owner
            || matches!(self.record.phase(), Phase::Stopping | Phase::Closed)
        {
            return Err(RunError::broken("invalid foreground child admission"));
        }
        let mut ancestors = self.ancestor_owners.clone();
        ancestors.push(self.record.incarnation().owner.clone());
        let mut gates = self._gate.share_custody()?;
        for gate in &self.ancestor_gates {
            gates.push(crate::gate::duplicate_custody(gate)?);
        }
        let mut child = Self::begin_under(&self.cx, root, inv, opts, gates, ancestors)?;
        let completion = Arc::new(Mutex::new(None));
        child.completion = Some(completion.clone());
        self.children.push(completion);
        Ok(child)
    }

    fn begin_under(
        cx: &EffectContext,
        root: &StateRoot,
        inv: &ResolvedInvocation,
        opts: BeginOpts,
        ancestor_gates: Vec<OwnedFd>,
        ancestor_owners: Vec<ResourceId>,
    ) -> Result<Self, RunError> {
        if cx.operation().placement() != Placement::Host || inv.spec().placement != Placement::Host
        {
            return Err(RunError::refused(
                "a managed run needs host authority, and this placement is a guest",
            ));
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
            return Err(RunError::refused(
                "foreground admission requires local checkout capabilities",
            ));
        }
        cx.operation().check("foreground admission")?;
        let plan = opts
            .custody
            .ok_or_else(|| RunError::broken("foreground custody needs a memory guard"))?;
        if plan.placement != crate::supervise::Placement::Inline {
            return Err(RunError::broken("foreground custody cannot detach"));
        }
        let mut ids = inv.spec().gates.clone();
        ids.push(inv.spec().owner.clone());
        if ids.iter().any(|id| {
            (id.scope() != inv.spec().owner.scope() && !id.checkout_alias_of(&inv.spec().owner))
                || !matches!(id.scope(), Scope::Checkout(_))
                || !matches!(id.kind(), ResourceKind::Task | ResourceKind::Store)
        }) {
            return Err(RunError::broken(
                "foreground gates must belong to the owning checkout",
            ));
        }
        let ids = aliases::gates(root, &inv.spec().owner, &ids)?;
        if ids.len() > crate::gate::FOREGROUND_GATES_MAX {
            return Err(RunError::refused(
                "a foreground admission holds at most the task's gate and one shared lock, \
                 with each one's checkout-alias spelling",
            ));
        }
        let mut gate = Gate::acquire(cx, root, &ids, opts.wait)?;
        // Declared shared exclusion gates remain held too; adapter gates
        // cannot reach this local-only path. The held flock is the exclusion: a
        // predecessor it does not name cannot still be live, so a journal row
        // left behind is not a reason to refuse (F11).
        gate.set_foreground(
            &opts.id,
            &inv.spec().owner,
            &inv.spec().task,
            ancestor_owners.is_empty(),
        );
        let guard = MemoryGuard::start(
            &mut gate,
            crate::supervise::memory::MemoryStart {
                executable: &plan.executable,
                ancestors: &ancestor_gates,
                cx,
                root,
                run: opts.id,
                fence: Fence {
                    resource: inv.spec().owner.clone(),
                    generation: Generation::new(0),
                },
                purpose: crate::supervise::memory::MemoryPurpose::Foreground,
                deadline_nanos: effigy_core::run_deadline::current()
                    .map(|deadline| deadline.ceiling_nanos()),
            },
        )?;
        let boot = crate::ident::boot_identity()
            .verified()
            .map(|boot| boot.as_str().to_owned())
            .unwrap_or_else(|| "unknown".to_owned());
        Ok(Self {
            root: root.clone(),
            task: inv.spec().task.task().to_owned(),
            boot,
            record: ForegroundRecord::new(opts.id, inv.spec().owner.clone()),
            cx: cx.clone(),
            next: opts.next,
            executable: plan.executable,
            guard: Some(guard),
            processes: BTreeMap::new(),
            _gate: gate,
            ancestor_gates,
            ancestor_owners,
            children: Vec::new(),
            completion: None,
            truncated: false,
        })
    }
    pub fn record(&self) -> &ForegroundRecord {
        &self.record
    }
    /// The owning engine's mutable record handle. The record's own methods
    /// keep the custody invariants; this accessor only lends it.
    pub fn record_mut(&mut self) -> &mut ForegroundRecord {
        &mut self.record
    }
    pub fn guard_identity(&self) -> Option<&ProcessIdentity> {
        self.guard.as_ref().and_then(|g| g.identity.as_ref())
    }
    pub(crate) fn closure_pin(&self) -> Result<crate::closure::ClosurePin, RunError> {
        self.guard
            .as_ref()
            .ok_or_else(|| RunError::broken("foreground guard unavailable"))?
            .closure_pin()
    }
    pub(crate) fn has_guard(&self) -> bool {
        self.guard.is_some()
    }
    /// The effects this run has not released, in the kernel's own words, so a
    /// settle that cannot finish names what stays retained (effigy#992).
    pub fn retained_effects(&self) -> Vec<String> {
        self.record
            .effects()
            .iter()
            .filter(|effect| !effect.state.terminal())
            .map(|effect| format!("`{}` effect {}", effect.spec.kind, effect.sequence))
            .collect()
    }
    pub(crate) fn anchor_image(&self) -> Result<crate::supervise::anchor::Image, RunError> {
        crate::supervise::anchor::Image::prepare(&self.executable)
    }
    pub(crate) fn ack_process(&mut self, ticket: &EffectTicket) -> Result<(), RunError> {
        let effect = self
            .record
            .effects()
            .iter()
            .find(|e| e.sequence == ticket.sequence)
            .ok_or_else(|| RunError::broken("missing foreground process receipt"))?;
        self.guard
            .as_mut()
            .ok_or_else(|| RunError::broken("foreground guard unavailable"))?
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
        if workload == WorkloadOutcome::Succeeded
            && self.children.iter().any(|child| {
                *child
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    != Some(WorkloadOutcome::Succeeded)
            })
        {
            return Err(RunError::broken("foreground child obligations incomplete"));
        }
        self.record.finish(workload)
    }
    pub fn interrupt(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        self.record.interrupt(workload)
    }
    /// Settles every effect in reverse spawn order and closes the run.
    ///
    /// If the invocation's absolute run deadline cuts settlement short, the
    /// run records that fact, keeps its surviving custodian through the
    /// bounded attempt, and reports the interrupted outcome rather than
    /// pretending to have settled (effigy#992). Under the opt-in any
    /// settlement that is not positively complete -- an error, a timeout or
    /// residue -- is recorded as incomplete, never as success.
    pub fn settle(mut self, cleanup: &CleanupContext) -> Result<Outcome<Settled>, RunError> {
        let result = self.settle_inner(cleanup);
        if effigy_core::run_deadline::ceiling().is_some() && result.is_err() {
            // A settle that failed for any reason under the deadline leaves
            // the run unsettled: the guard is left as the surviving custodian
            // and is not closed or waited for. Its owner-death settlement
            // keeps the gates and settles residue after our EOF.
            self.truncated = true;
            self.abandon_guard();
            effigy_core::run_deadline::note_settlement_incomplete();
        }
        result
    }

    /// Leave the memory guard to the owner-death settlement it already owns.
    /// The kernel never kills or closes it here: it releases its own pins,
    /// returns at once, and the guard process settles residue with the gates
    /// it holds.
    fn abandon_guard(&mut self) {
        if let Some(guard) = self.guard.as_mut() {
            guard.abandon();
        }
    }

    fn settle_inner(&mut self, cleanup: &CleanupContext) -> Result<Outcome<Settled>, RunError> {
        let effects = self.record.effects().to_vec();
        for effect in effects.iter().rev() {
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
        let outcome = settle::outcome(
            self.record.incarnation().run,
            &self.next,
            self.record
                .workload()
                .cloned()
                .unwrap_or(WorkloadOutcome::Interrupted),
            self.record.effects().to_vec(),
        )?;
        // Shared custody disables GateFile's ordinary explicit unlock. Restore
        // it only after complete effects, positive descendant closure and the
        // exact memory guard's successful exit below. A concurrent unrelated
        // fork may still hold CLOEXEC copies until exec; those are no longer
        // custody and must not make a settled foreground run appear live.
        let descendants_closed = self.children.iter().all(|child| {
            child
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .is_some()
        });
        let effects_complete = outcome.cleanup() != CleanupOutcome::Incomplete;
        let mut outcome = outcome;
        if !descendants_closed && effigy_core::run_deadline::ceiling().is_some() {
            // Under the opt-in a child run that never reported positive
            // completion leaves this tree unsettled. It is residue, not
            // success, and it is carried into the outcome, the terminal
            // record and the footer decision. Without the opt-in the previous
            // behaviour is unchanged.
            outcome.add_residue(crate::outcome::Residue::unresolved(
                0,
                "in-process child",
                self.record.incarnation().owner.clone(),
                Some("foreground child obligation"),
                "a child run did not report positive completion",
                self.next.clone(),
            ));
        }
        let incomplete = outcome.cleanup() == CleanupOutcome::Incomplete;
        if incomplete && effigy_core::run_deadline::ceiling().is_some() {
            // The owner keeps its handles and runs positive settlement right
            // up to the ceiling. Reaching it unsettled records the run as
            // incomplete; nothing here claims closure.
            self.truncated = true;
            effigy_core::run_deadline::note_settlement_incomplete();
        }
        // Always attempt the guard's positive close, bounded by the cleanup
        // context (which the run deadline already caps). An error leaves the
        // guard field for `settle` to abandon without a second wait;
        // a successful close is the only case where the guard is taken.
        if let Some(guard) = self.guard.as_mut() {
            guard.close(cleanup)?;
        }
        self.guard.take();
        let Scope::Checkout(checkout) = self.record.incarnation().owner.scope() else {
            unreachable!("foreground admission requires a checkout");
        };
        let (state, exit_code, signal) = match outcome.workload() {
            WorkloadOutcome::NotStarted => ("not-started", None, None),
            WorkloadOutcome::Succeeded => ("succeeded", Some(0), None),
            WorkloadOutcome::Failed { code } => ("failed", Some(*code), None),
            WorkloadOutcome::Cancelled { signal, .. } => ("cancelled", None, *signal),
            WorkloadOutcome::TimedOut { .. } => ("timed-out", None, None),
            WorkloadOutcome::Interrupted => ("interrupted", None, None),
            WorkloadOutcome::Unknown(_) => ("unknown", None, None),
        };
        let record = TerminalRecord {
            run_id: self.record.incarnation().run.to_string(),
            checkout: checkout.text(),
            boot: self.boot.clone(),
            task: self.task.clone(),
            outcome: state.to_owned(),
            exit_code,
            signal,
            cleanup: if incomplete { "incomplete" } else { "complete" }.to_owned(),
            closed_at_ms: crate::store::wall_ms(),
        };
        if let Err(error) = terminal::record(
            &cleanup.operation().effects().expect("cleanup has effects"),
            &self.root,
            &record,
        ) {
            outcome.add_warning(crate::outcome::Warning {
                code: "terminal_record".into(),
                message: format!("could not record foreground outcome: {error}"),
            });
        }
        if effects_complete && descendants_closed {
            self._gate.release_closed_custody();
            if let Some(completion) = &self.completion {
                *completion
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner) =
                    Some(outcome.workload().clone());
            }
        }
        Ok(outcome)
    }
}

impl Drop for ForegroundRun {
    fn drop(&mut self) {
        // A settle the run deadline cut short keeps its surviving custodian:
        // the memory guard settles residue after its owner exits, and this
        // drop must not signal, close, or discard it and then imply closure.
        if self.truncated {
            return;
        }
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
