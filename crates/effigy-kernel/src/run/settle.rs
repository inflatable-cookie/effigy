use super::*;
use crate::observe::{Evidence, Observed};
use crate::outcome::{
    CleanupOutcome, EffectDisposition, EffectReport, ErrorCode, ErrorInfo, Residue,
};
use crate::store::{BusyWait, SETTLE_BUSY_WAIT};
use std::time::Duration;

/// Engine implementations must observe exact identity and make undo idempotent.
/// No transaction or auxiliary resource gate spans these callbacks (K14).
pub trait SettlementAdapter {
    /// Same selected backend used by launch, also available in sentinel recovery.
    fn remote(&mut self) -> Option<&mut dyn crate::remote::RemoteExecAdapter> {
        None
    }
    fn observe(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Fence>;
    fn release(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<()>;
    /// Policy can retain a borrowed resource. Returning Verified(Retained) does
    /// not grant authority to stop an environment; that requires its own run.
    fn reconcile(&mut self, effect: &Effect, cx: &CleanupContext) -> Observed<Reconciled>;
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reconciled {
    Released,
    Retained,
    /// The engine could not finish undoing what it owns and says why, by name.
    /// The effect settles as `Retained` with this reason: it is terminal (I-R7),
    /// so a residue the engine cannot act on never blocks the owner's next run,
    /// and the reason stays on the effect for the operator (§5.6).
    Residue(String),
}
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Settled {
    pub run: RunId,
}

pub(super) fn cleanup_effects(cleanup: &CleanupContext) -> Result<EffectContext, RunError> {
    cleanup.operation().check("settle")?;
    cleanup
        .operation()
        .effects()
        .map_err(|_| RunError::broken("cleanup needs effects"))
}

/// Reclaim completed machine rows left between the close commit and sentinel
/// exit. The owner gate excludes a live owner; the row remains visible until
/// the exact sentinel is positively gone, including while it is stopped.
pub(super) fn compact_machine_closed(
    store: &mut Store,
    owner: &ResourceId,
    cleanup: &CleanupContext,
) -> Result<(), RunError> {
    use crate::ident::{self, ProcessIdentity, ProcessState};
    use crate::observe::Observed;

    if owner.kind() == crate::id::ResourceKind::Gateway {
        return Ok(());
    }
    loop {
        cleanup
            .operation()
            .check("compact completed machine runs")?;
        let rows = {
            let mut query = store.conn().prepare("SELECT run_id,sentinel_identity FROM runs WHERE owner=?1 AND phase='closed' AND cleanup_outcome='complete' AND sentinel_identity IS NOT NULL ORDER BY generation LIMIT 32")?;
            let rows = query
                .query_map([owner.name().to_string()], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        if rows.is_empty() {
            return Ok(());
        }
        for (_, encoded) in &rows {
            let sentinel: ProcessIdentity = serde_json::from_str(encoded)?;
            loop {
                cleanup.operation().check("completed sentinel exit")?;
                match ident::process_identity(sentinel.pid) {
                    Observed::Absent(_) => break,
                    Observed::Verified(actual, _) if actual != sentinel => break,
                    Observed::Unknown(reason) => {
                        return Err(RunError::unavailable(format!(
                            "completed sentinel identity: {reason}"
                        )))
                    }
                    Observed::Verified(_, _) => match ident::process_state(sentinel.pid) {
                        Observed::Absent(_) => break,
                        Observed::Verified(ProcessState::Zombie, _) => {
                            // Recheck identity so a reused PID cannot lend its
                            // zombie state to the recorded sentinel.
                            match ident::process_identity(sentinel.pid) {
                                Observed::Verified(_, _) | Observed::Absent(_) => break,
                                Observed::Unknown(reason) => {
                                    return Err(RunError::unavailable(format!(
                                        "completed sentinel identity: {reason}"
                                    )))
                                }
                            }
                        }
                        Observed::Verified(_, _) => {}
                        Observed::Unknown(reason) => {
                            return Err(RunError::unavailable(format!(
                                "completed sentinel state: {reason}"
                            )))
                        }
                    },
                }
                std::thread::sleep(
                    Duration::from_millis(5)
                        .min(cleanup.operation().remaining().unwrap_or_default()),
                );
            }
        }
        let cx = cleanup_effects(cleanup)?;
        store.transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
        for (id, _) in &rows {
            txn.conn().execute("DELETE FROM runs WHERE run_id=?1 AND owner=?2 AND phase='closed' AND cleanup_outcome='complete'", params![id, owner.name().to_string()])?;
        }
        Ok::<_, RunError>(())
    })?;
    }
}
impl Run<'_> {
    pub(super) fn settle_inner(
        &mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Outcome<Settled>, RunError> {
        self.settle_recovering(cleanup, adapter, None)
    }

    /// Keyed admission already bound its run and advanced the owner fence.
    /// Keep its predecessor's escalation and the current successor's mailbox
    /// serviceable through this owner-gated recovery, without accepting stale
    /// client requests or borrowing another run's authority.
    pub(super) fn settle_recovering(
        &mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        successor: Option<(RunId, Generation)>,
    ) -> Result<Outcome<Settled>, RunError> {
        let cx = cleanup_effects(cleanup)?;
        if !self.guard.covers(&self.owner.resource) {
            return Err(RunError::broken("a settle without its owner gate"));
        }
        crate::fault_point!(SettleBefore)?;
        let record = journal::run(self.store.conn(), self.id)?;
        let workload = record.workload.unwrap_or(WorkloadOutcome::Interrupted);
        let machine = self.root.is_machine();
        let batch = self.machine_lifecycle();
        if machine
            && self.owner.resource.kind() == crate::id::ResourceKind::Gateway
            && record.phase != Phase::Closed
        {
            if let Some(effigy_home) = self.root.path().parent() {
                record_gateway_stop(
                    &effigy_home.join("gateway"),
                    &format!("custody settlement: {workload}"),
                );
            }
        }
        let mut dispositions = std::collections::BTreeMap::new();
        let mut stderr_tail = String::new();
        if record.phase != Phase::Closed && record.phase != Phase::Stopping && !machine {
            self.store.transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
                txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",
                    params![self.id.to_string(),serde_json::to_string(&workload)?,txn.now()])?;
                Ok::<_,RunError>(())
            })?;
        }
        crate::fault_point!(SettleStopping)?;
        let effects = self.effects()?;
        // Reverse effect order (K4), attachments last: an attachment is
        // released only once nothing else of the run is open, so a holder's
        // own effects, including those it intended before attaching, settle
        // first ("then release attachments", §10).
        let open = |attachment: bool| {
            effects.iter().rev().filter(move |effect| {
                !effect.state.terminal() && (effect.spec.kind == attachments::KIND) == attachment
            })
        };
        for effect in open(false).chain(open(true)) {
            cleanup.operation().check("settle effect")?;
            crate::fault_point!(SettleEffectBefore)?;
            if effect.spec.kind == children::KIND {
                self.settle_child(effect, cleanup, adapter)?;
            } else if effect.spec.kind == crate::objects::KIND {
                self.settle_staging(effect, &cx)?;
            } else if effect.spec.kind == socket::KIND {
                if batch {
                    dispositions.insert(effect.sequence, self.socket_disposition(effect));
                } else {
                    self.settle_socket(effect, &cx)?;
                }
            } else if effect.spec.kind == crate::custody::KIND {
                let live = self.processes.remove(&effect.sequence);
                let diagnostic = live.clone();
                // Stop can be inside its grace wait when a front end sends its
                // second interrupt. Read the fenced durable mailbox here too;
                // no transaction or new writer spans process I/O (K4/K14).
                let (state, reason) = crate::custody::settle_effect(effect, live, cleanup, || {
                    if let Some((run, generation)) = successor {
                        return self.recovery_escalated(run, generation).unwrap_or(false);
                    }
                    self.requests().is_ok_and(|requests| {
                        requests
                            .iter()
                            .any(|r| matches!(r.action, RequestAction::Escalate))
                    })
                });
                if let Some(core) = diagnostic {
                    stderr_tail = crate::custody::diagnostic_tail(&core);
                }
                if batch {
                    dispositions.insert(effect.sequence, (state, reason));
                } else {
                    self.store.transaction_within(
                        &cx,
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
                    )?;
                }
            } else if effect.spec.kind == crate::remote::KIND {
                let (state, reason) =
                    crate::remote::settle_effect(effect, cleanup, adapter.remote());
                self.store
                    .transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
                        journal::write_effect(
                            txn,
                            self.id,
                            effect.sequence,
                            state,
                            None,
                            reason.as_deref(),
                        )
                    })?;
            } else if effect.spec.kind == attachments::KIND {
                self.settle_attachment(effect, &cx)?;
            } else {
                // Recording a retained or irreversible disposition calls no
                // adapter, so holder protection (R4) has nothing to guard.
                let records_only = matches!(
                    effect.spec.settlement,
                    Settlement::Retain | Settlement::Irreversible
                );
                let (state, reason) = if effect.spec.target.resource.kind()
                    == crate::id::ResourceKind::Env
                    && !records_only
                {
                    match attachments::ensure_no_holders(
                        self.store.conn(),
                        &effect.spec.target.resource,
                    ) {
                        Ok(()) if self.guard.covers(&effect.spec.target.resource) => {
                            resolve(adapter, effect, cleanup)
                        }
                        _ => (
                            EffectState::Residue,
                            Some("environment holders or missing environment gate".into()),
                        ),
                    }
                } else {
                    resolve(adapter, effect, cleanup)
                };
                self.store
                    .transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
                        journal::write_effect(
                            txn,
                            self.id,
                            effect.sequence,
                            state,
                            None,
                            reason.as_deref(),
                        )
                    })?;
            }
            crate::fault_point!(SettleEffectAfter)?;
        }
        let mut effects = self.effects()?;
        for effect in &mut effects {
            if let Some((state, reason)) = dispositions.get(&effect.sequence) {
                effect.state = *state;
                effect.reason = reason.clone();
            }
        }
        let complete = effects.iter().all(|e| e.state.terminal());
        // A non-gateway sentinel needs the closed row as positive evidence
        // until it has observed EOF and exited. Gateway has its own bounded
        // completion fact in the resource row.
        let delete_after_sentinel = machine
            && complete
            && self.sentinel.is_some()
            && self.owner.resource.kind() != crate::id::ResourceKind::Gateway;
        // The close write keeps the store's own two-minute window, so a busy
        // store delays it rather than losing the run's outcome. A write that
        // still cannot land leaves the run unsettled: the sentinel or the next
        // holder of this gate reconciles it, and the caller reports a warning,
        // never a failed task. ENOSPC at Closed retains liveness until commit
        // succeeds or the cleanup deadline expires; the caller never mistakes
        // a missing close for success.
        loop {
            let result = (|| {
                crate::fault_point!(SettleCloseBefore)?;
                self.store.transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
                    for (sequence, (state, reason)) in &dispositions {
                        journal::write_effect(txn, self.id, *sequence, *state, None, reason.as_deref())?;
                    }
                    txn.conn().execute("UPDATE runs SET phase='closed',cleanup_outcome=?2,updated_at=?3,closed_at=coalesce(closed_at,?3) WHERE run_id=?1",
                        params![self.id.to_string(),if complete { "complete" } else { "incomplete" },txn.now()])?;
                    // A keyed workspace request's final answer and every
                    // client attachment end in this same commit (§8.11).
                    workspace::closed(txn, self.id, &workload, complete)?;
                    if machine && complete {
                        if self.owner.resource.kind() == crate::id::ResourceKind::Gateway {
                            let completion = MachineCompletion { run: self.id, generation: self.owner.generation, workload: workload.clone(), stderr_tail: stderr_tail.clone() };
                            txn.conn().execute("UPDATE resources SET state=?2,updated_at=?3 WHERE name=?1 AND generation=?4", params![self.owner.resource.name().to_string(),serde_json::to_string(&completion)?,txn.now(),self.owner.generation.get() as i64])?;
                        }
                        if !delete_after_sentinel {
                            txn.conn().execute("DELETE FROM runs WHERE run_id=?1", [self.id.to_string()])?;
                        }
                    }
                    Ok::<_,RunError>(())
                })?;
                crate::fault_point!(SettleCloseAfter)?;
                Ok::<_, RunError>(())
            })();
            match result {
                Ok(()) => break,
                Err(RunError::Store(StoreError::Io(_))) => {
                    cleanup.operation().check("settle close")?;
                    std::thread::sleep(
                        Duration::from_millis(5)
                            .min(cleanup.operation().remaining().unwrap_or_default()),
                    );
                }
                Err(error) => return Err(error),
            }
        }
        if machine && complete {
            self.closed_effects = Some(effects.clone());
        }
        // K4: only Run closes the owner-only sentinel writer, after the Closed
        // transaction and while its owner gate is still held.
        self.close_checkout_custody(cleanup)?;
        if let Some(mut sentinel) = self.sentinel.take() {
            sentinel.close(cleanup)?;
        }
        if delete_after_sentinel {
            self.store.transaction_within(&cx, BusyWait::AtLeast(SETTLE_BUSY_WAIT), |txn| {
                txn.conn().execute("DELETE FROM runs WHERE run_id=?1 AND phase='closed' AND cleanup_outcome='complete'", [self.id.to_string()])?;
                Ok::<_, RunError>(())
            })?;
        }
        outcome(self.id, &self.next, workload, effects)
    }
}

pub(super) fn outcome(
    id: RunId,
    next: &Invocation,
    workload: WorkloadOutcome,
    effects: Vec<Effect>,
) -> Result<Outcome<Settled>, RunError> {
    let complete = effects.iter().all(|e| e.state.terminal());
    let cleanup_result = if complete {
        CleanupOutcome::Complete
    } else {
        CleanupOutcome::Incomplete
    };
    let result = if complete {
        Ok(Settled { run: id })
    } else {
        Err(ErrorInfo {
            code: ErrorCode::Incomplete,
            message: "cleanup has unresolved effects".into(),
            details: Value::Null,
        })
    };
    let mut outcome = Outcome::new(result, workload, cleanup_result, &[])
        .map_err(|_| RunError::broken("stored workload outcome"))?;
    for effect in effects {
        outcome.add_effect(EffectReport {
            resource: effect.spec.target.resource.clone(),
            generation: effect.spec.target.generation,
            disposition: match effect.state {
                EffectState::Released => EffectDisposition::Released,
                EffectState::Retained => EffectDisposition::Retained,
                EffectState::Irreversible => EffectDisposition::Irreversible,
                EffectState::Failed => EffectDisposition::Failed,
                _ => EffectDisposition::Unknown,
            },
        });
        if !effect.state.terminal() {
            outcome.add_residue(Residue {
                kind: effect.spec.kind,
                id: effect.spec.target.resource,
                observation: Observed::Unknown(UnknownReason::Inconsistent(
                    effect.reason.unwrap_or_else(|| "unresolved effect".into()),
                )),
                next: next.clone(),
            });
        }
    }
    if !complete {
        outcome.add_next(next.clone());
    }
    Ok(outcome)
}

fn resolve(
    adapter: &mut impl SettlementAdapter,
    effect: &Effect,
    cx: &CleanupContext,
) -> (EffectState, Option<String>) {
    match effect.spec.settlement {
        Settlement::Retain => return (EffectState::Retained, None),
        Settlement::Irreversible => return (EffectState::Irreversible, None),
        _ => {}
    }
    #[cfg(all(test, feature = "fault-injection"))]
    let observed = {
        let observed = crate::fault::injected().unwrap_or_else(|| adapter.observe(effect, cx));
        crate::fault::record(crate::fault::Event::Observed {
            subject: subject(effect),
            unknown: matches!(observed, Observed::Unknown(_)),
        });
        observed
    };
    #[cfg(not(all(test, feature = "fault-injection")))]
    let observed = adapter.observe(effect, cx);
    match observed {
        Observed::Absent(_) => return (EffectState::Released, None),
        Observed::Unknown(reason) => return residue(reason),
        Observed::Verified(owner, _) if owner != effect.spec.target => {
            return (EffectState::Released, None)
        }
        Observed::Verified(_, _) => {}
    }
    if let Err(reason) = cx.operation().check("settle adapter") {
        return (EffectState::Residue, Some(format!("{reason}")));
    }
    if let Err(error) = crate::fault_point!(AdapterBefore) {
        return (EffectState::Residue, Some(error.to_string()));
    }
    #[cfg(all(test, feature = "fault-injection"))]
    crate::fault::record(crate::fault::Event::Destructive {
        subject: subject(effect),
    });
    let result = match effect.spec.settlement {
        Settlement::Release(_) => adapter.release(effect, cx).map(|()| Reconciled::Released),
        Settlement::Reconcile(_) => adapter.reconcile(effect, cx),
        _ => unreachable!("non-adapter settlements returned above"),
    };
    if let Err(error) = crate::fault_point!(AdapterAfter) {
        return (EffectState::Residue, Some(error.to_string()));
    }
    match result {
        Observed::Verified(Reconciled::Released, _) | Observed::Absent(_) => {
            (EffectState::Released, None)
        }
        Observed::Verified(Reconciled::Retained, _) => (EffectState::Retained, None),
        Observed::Verified(Reconciled::Residue(reason), _) => (EffectState::Retained, Some(reason)),
        Observed::Unknown(reason) => residue(reason),
    }
}
/// O8 subject: the exact fenced effect an observation or undo concerned.
#[cfg(all(test, feature = "fault-injection"))]
pub(crate) fn subject(effect: &Effect) -> String {
    format!(
        "{}@{}:{}",
        effect.spec.kind,
        effect.spec.target.generation.get(),
        effect.spec.payload
    )
}
fn residue(reason: UnknownReason) -> (EffectState, Option<String>) {
    (EffectState::Residue, Some(format!("{reason}")))
}

/// Recovery serializes with both a live owner and another recovery attempt.
/// An absent run is an error, never evidence of settlement (R5).
pub fn settle_interrupted(
    cx: &EffectContext,
    root: &StateRoot,
    owner: &ResourceId,
    id: RunId,
    next: Invocation,
    cleanup: &CleanupContext,
    adapter: &mut impl SettlementAdapter,
) -> Result<Outcome<Settled>, RunError> {
    settle_interrupted_wait(
        cx,
        root,
        (owner, id),
        next,
        cleanup,
        adapter,
        Wait::FailFast,
    )
}

// Sentinel EOF uses the same sole writer with a finite owner-gate wait (§5.8).
pub(crate) fn settle_interrupted_wait(
    cx: &EffectContext,
    root: &StateRoot,
    target: (&ResourceId, RunId),
    next: Invocation,
    cleanup: &CleanupContext,
    adapter: &mut impl SettlementAdapter,
    wait: Wait,
) -> Result<Outcome<Settled>, RunError> {
    let (owner, id) = target;
    let ids = super::aliases::gates(root, owner, std::slice::from_ref(owner))?;
    let guard = Gate::acquire(cx, root, &ids, wait)?;
    let store = Store::open(&cleanup_effects(cleanup)?, root)?;
    let record = journal::run(store.conn(), id)?;
    if record.owner != owner.name().to_string() {
        return Err(RunError::broken("wrong run owner"));
    }
    Run {
        root,
        store,
        guard,
        cx: cx.clone(),
        id,
        owner: Fence {
            resource: owner.clone(),
            generation: record.generation,
        },
        next,
        poisoned: false,
        completed: Default::default(),
        sentinel: None,
        closed_effects: None,
        machine_startup: None,
        checkout_custody: None,
        ancestor_closure: None,
        processes: Default::default(),
    }
    .settle(cleanup, adapter)
}
pub(super) fn evidence() -> Evidence {
    Evidence::new("kernel run journal".into(), None).expect("fixed evidence")
}
