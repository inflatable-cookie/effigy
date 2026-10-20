use super::*;
use crate::observe::{Evidence, Observed};
use crate::outcome::{
    CleanupOutcome, EffectDisposition, EffectReport, ErrorCode, ErrorInfo, Residue,
};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconciled {
    Released,
    Retained,
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
        .map_err(|_| RunError::Invalid("cleanup needs effects".into()))
}
impl Run<'_> {
    pub(super) fn settle_inner(
        &mut self,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
    ) -> Result<Outcome<Settled>, RunError> {
        let cx = cleanup_effects(cleanup)?;
        if !self.guard.covers(&self.owner.resource) {
            return Err(RunError::Invalid("settle needs owner gate".into()));
        }
        crate::fault_point!(SettleBefore)?;
        let record = journal::run(self.store.conn(), self.id)?;
        let workload = record.workload.unwrap_or(WorkloadOutcome::Interrupted);
        if record.phase != Phase::Closed {
            self.store.transaction(&cx, |txn| {
                txn.conn().execute("UPDATE runs SET phase='stopping',workload_outcome=?2,updated_at=?3 WHERE run_id=?1",
                    params![self.id.to_string(),serde_json::to_string(&workload)?,txn.now()])?;
                Ok::<_,RunError>(())
            })?;
        }
        crate::fault_point!(SettleStopping)?;
        let effects = self.effects()?;
        for effect in effects
            .iter()
            .rev()
            .filter(|effect| !effect.state.terminal())
        {
            cleanup.operation().check("settle effect")?;
            crate::fault_point!(SettleEffectBefore)?;
            if effect.spec.kind == crate::objects::KIND {
                self.settle_staging(effect, &cx)?;
            } else if effect.spec.kind == socket::KIND {
                self.settle_socket(effect, &cx)?;
            } else if effect.spec.kind == crate::custody::KIND {
                let live = self.processes.remove(&effect.sequence);
                // Stop can be inside its grace wait when a front end sends its
                // second interrupt. Read the fenced durable mailbox here too;
                // no transaction or new writer spans process I/O (K4/K14).
                let (state, reason) = crate::custody::settle_effect(effect, live, cleanup, || {
                    self.requests().is_ok_and(|requests| {
                        requests
                            .iter()
                            .any(|r| matches!(r.action, RequestAction::Escalate))
                    })
                });
                self.store.transaction(&cx, |txn| {
                    journal::write_effect(
                        txn,
                        self.id,
                        effect.sequence,
                        state,
                        None,
                        reason.as_deref(),
                    )
                })?;
            } else if effect.spec.kind == crate::remote::KIND {
                let (state, reason) =
                    crate::remote::settle_effect(effect, cleanup, adapter.remote());
                self.store.transaction(&cx, |txn| {
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
            } else if effect.spec.kind == claims::KIND {
                self.settle_claims(effect, &cx)?;
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
                self.store.transaction(&cx, |txn| {
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
        let effects = self.effects()?;
        let complete = effects.iter().all(|e| e.state.terminal());
        let cleanup_result = if complete {
            CleanupOutcome::Complete
        } else {
            CleanupOutcome::Incomplete
        };
        // ENOSPC at Closed retains liveness until commit succeeds or the cleanup
        // deadline expires. The caller never mistakes a missing close for success.
        loop {
            cleanup.operation().check("settle close")?;
            let result = (|| {
                crate::fault_point!(SettleCloseBefore)?;
                self.store.transaction(&cx, |txn| {
                    txn.conn().execute("UPDATE runs SET phase='closed',cleanup_outcome=?2,updated_at=?3,closed_at=coalesce(closed_at,?3) WHERE run_id=?1",
                        params![self.id.to_string(),if complete { "complete" } else { "incomplete" },txn.now()])?;
                    Ok::<_,RunError>(())
                })?;
                crate::fault_point!(SettleCloseAfter)?;
                Ok::<_, RunError>(())
            })();
            match result {
                Ok(()) => break,
                Err(RunError::Store(StoreError::Io(_) | StoreError::Busy)) => std::thread::sleep(
                    Duration::from_millis(5)
                        .min(cleanup.operation().remaining().unwrap_or_default()),
                ),
                Err(error) => return Err(error),
            }
        }
        // K4: only Run closes the owner-only sentinel writer, after the Closed
        // transaction and while its owner gate is still held.
        if let Some(mut sentinel) = self.sentinel.take() {
            sentinel.close(cleanup)?;
        }
        let result = if complete {
            Ok(Settled { run: self.id })
        } else {
            Err(ErrorInfo {
                code: ErrorCode::Incomplete,
                message: "cleanup has unresolved effects".into(),
                details: Value::Null,
            })
        };
        let mut outcome = Outcome::new(result, workload, cleanup_result, &[])
            .map_err(|_| RunError::Invalid("stored workload outcome".into()))?;
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
                    next: self.next.clone(),
                });
            }
        }
        if !complete {
            outcome.add_next(self.next.clone());
        }
        Ok(outcome)
    }
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
        return (EffectState::Residue, Some(format!("{reason:?}")));
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
    (EffectState::Residue, Some(format!("{reason:?}")))
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
    let guard = Gate::acquire(cx, root, std::slice::from_ref(owner), wait)?;
    let store = Store::open(&cleanup_effects(cleanup)?, root)?;
    let record = journal::run(store.conn(), id)?;
    if record.owner != owner.name().to_string() {
        return Err(RunError::Invalid("wrong run owner".into()));
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
        processes: Default::default(),
    }
    .settle(cleanup, adapter)
}
pub(super) fn evidence() -> Evidence {
    Evidence::new("kernel run journal".into(), None).expect("fixed evidence")
}
