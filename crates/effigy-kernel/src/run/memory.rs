//! Foreground intent and completion live only in this record (052 §3.1).
//!
//! This is not a recoverable journal. Custodians must retain their process
//! receipts and owner gates until closure; losing both custodians loses this
//! record. No store, filesystem, clock, or process API is available here.
use super::{Effect, EffectSpec, EffectState, EffectTicket, Fence, Phase, RunError, Settlement};
use crate::id::{Generation, ResourceId, RunId};
use crate::observe::{AbsenceProof, UnknownReason};
use crate::outcome::{Outcome, RequiredObligation, WorkloadOutcome};
use serde_json::Value;
use std::collections::BTreeMap;

/// Generation zero is local to this RunId, never a persistent generation.
/// A resource name or the zero generation alone cannot address an incarnation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Incarnation {
    pub run: RunId,
    pub owner: ResourceId,
}

/// The memory half of foreground custody. It cannot acquire an attachment,
/// claim, remote session, retained effect, or any other durable capability.
/// The engine selects durability before creating this record; attempts to add
/// a durable effect later are refused, with the record left unchanged.
#[derive(Debug)]
pub struct ForegroundRecord {
    incarnation: Incarnation,
    fence: Fence,
    phase: Phase,
    effects: Vec<Effect>,
    completed: BTreeMap<i64, RequiredObligation>,
    workload: Option<WorkloadOutcome>,
}

impl ForegroundRecord {
    /// The caller holds the checkout's owner gate and has ruled out recorded
    /// unresolved predecessors before admission. IDs require fresh entropy.
    pub fn new(run: RunId, owner: ResourceId) -> Self {
        Self {
            incarnation: Incarnation {
                run,
                owner: owner.clone(),
            },
            fence: Fence {
                resource: owner,
                generation: Generation::new(0),
            },
            phase: Phase::Admitted,
            effects: Vec::new(),
            completed: BTreeMap::new(),
            workload: Some(WorkloadOutcome::NotStarted),
        }
    }

    pub fn incarnation(&self) -> &Incarnation {
        &self.incarnation
    }

    /// Only valid together with `incarnation()`, and only inside this record.
    pub fn fence(&self) -> &Fence {
        &self.fence
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }

    pub fn workload(&self) -> Option<&WorkloadOutcome> {
        self.workload.as_ref()
    }

    /// Transient engine effects cannot address other resource generations or
    /// promise retention. Reserved kernel capabilities have dedicated paths.
    pub fn intend(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        if spec.kind.starts_with("kernel:") {
            return Err(invalid("reserved foreground effect kind"));
        }
        self.insert(spec)
    }

    /// Records only a local process intent. This grants no spawn authority:
    /// custody still requires a live guard and the receipt/ack barrier. Remote
    /// custody has no memory entry point; it requires a checkout journal.
    pub fn intend_process(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        if spec.kind != crate::custody::KIND {
            return Err(invalid("foreground custody needs a local process intent"));
        }
        self.insert(spec)
    }

    fn insert(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        self.can_effect()?;
        if spec.target != self.fence
            || !matches!(
                spec.settlement,
                Settlement::Release(_) | Settlement::Reconcile(_)
            )
        {
            return Err(invalid(
                "effect requires durable admission before workload go",
            ));
        }
        let sequence = i64::try_from(self.effects.len())
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| invalid("foreground effect sequence exhausted"))?;
        if spec.kind == crate::custody::KIND {
            self.workload = None;
        }
        self.effects.push(Effect {
            sequence,
            spec,
            state: EffectState::Intended,
            receipt: None,
            reason: None,
        });
        if self.phase == Phase::Admitted {
            self.phase = Phase::Starting;
        }
        Ok(EffectTicket {
            run: self.incarnation.run,
            owner: self.fence.clone(),
            sequence,
        })
    }

    pub fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
        self.callback(ticket, EffectState::Applied, Some(receipt), None)
    }

    pub fn failed(&mut self, ticket: &EffectTicket, proof: AbsenceProof) -> Result<(), RunError> {
        self.callback(
            ticket,
            EffectState::Failed,
            Some(serde_json::to_value(proof)?),
            None,
        )
    }

    pub fn unknown(
        &mut self,
        ticket: &EffectTicket,
        reason: UnknownReason,
    ) -> Result<(), RunError> {
        self.callback(
            ticket,
            EffectState::Unknown,
            None,
            Some(format!("{reason:?}")),
        )
    }

    fn callback(
        &mut self,
        ticket: &EffectTicket,
        state: EffectState,
        receipt: Option<Value>,
        reason: Option<String>,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        let effect = self.ticket_effect(ticket)?;
        if !matches!(effect.state, EffectState::Intended | EffectState::Unknown) {
            return Err(invalid("foreground effect already completed"));
        }
        effect.state = state;
        effect.receipt = receipt;
        effect.reason = reason;
        Ok(())
    }

    pub fn completed(
        &mut self,
        ticket: &EffectTicket,
        obligation: RequiredObligation,
    ) -> Result<(), RunError> {
        self.can_effect()?;
        if self.completed.contains_key(&ticket.sequence) {
            return Err(invalid("foreground effect already completed"));
        }
        let effect = self.ticket_effect(ticket)?;
        if effect.state != EffectState::Applied {
            return Err(invalid("foreground effect is not awaiting completion"));
        }
        if let RequiredObligation::ProcessExit(code) = obligation {
            if let Some(Value::Object(receipt)) = &mut effect.receipt {
                receipt.insert("exit".into(), Value::from(code));
            }
        }
        self.completed.insert(ticket.sequence, obligation);
        Ok(())
    }

    pub fn advance(&mut self, phase: Phase) -> Result<(), RunError> {
        self.can_effect()?;
        if !matches!(
            (self.phase, phase),
            (Phase::Starting, Phase::Active) | (_, Phase::Stopping)
        ) {
            return Err(invalid("invalid foreground phase transition"));
        }
        if phase == Phase::Active
            && self
                .effects
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(invalid("foreground startup obligations unresolved"));
        }
        self.phase = phase;
        Ok(())
    }

    /// Receipt of a spawn proves application, never natural completion.
    pub fn finish(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        self.can_effect()?;
        let obligations: Vec<_> = self
            .effects
            .iter()
            .filter(|e| e.spec.required)
            .map(|e| {
                self.completed
                    .get(&e.sequence)
                    .copied()
                    .unwrap_or(RequiredObligation::NotCompleted)
            })
            .collect();
        Outcome::new(
            Ok(()),
            workload.clone(),
            crate::outcome::CleanupOutcome::NotNeeded,
            &obligations,
        )
        .map_err(|_| invalid("foreground workload obligations incomplete"))?;
        self.workload = Some(workload);
        self.phase = Phase::Stopping;
        Ok(())
    }

    pub fn interrupt(&mut self, workload: WorkloadOutcome) -> Result<(), RunError> {
        if self.phase == Phase::Closed
            || !matches!(
                workload,
                WorkloadOutcome::Cancelled { .. }
                    | WorkloadOutcome::TimedOut { .. }
                    | WorkloadOutcome::NotStarted
            )
        {
            return Err(invalid("invalid foreground interruption outcome"));
        }
        self.workload = Some(workload);
        self.phase = Phase::Stopping;
        Ok(())
    }

    /// A custodian closes only after all effects have positive disposition or
    /// explicitly reported residue. This does not persist completion history.
    pub fn settle_effect(
        &mut self,
        sequence: i64,
        state: EffectState,
        reason: Option<String>,
    ) -> Result<(), RunError> {
        if !matches!(state, EffectState::Released | EffectState::Residue) {
            return Err(invalid("invalid foreground settlement disposition"));
        }
        if self.phase == Phase::Closed {
            return Err(invalid("foreground record is closed"));
        }
        let effect = self.effect_mut(sequence)?;
        if effect.state.terminal() {
            return Err(invalid("foreground effect already settled"));
        }
        effect.state = state;
        effect.reason = reason;
        self.phase = Phase::Stopping;
        Ok(())
    }

    pub fn close(&mut self) -> Result<(), RunError> {
        if self
            .effects
            .iter()
            .any(|e| !e.state.terminal() && e.state != EffectState::Residue)
        {
            return Err(invalid("foreground effects still open"));
        }
        if self.workload.is_none() {
            self.workload = Some(WorkloadOutcome::Interrupted);
        }
        self.phase = Phase::Closed;
        Ok(())
    }

    fn ticket_effect(&mut self, ticket: &EffectTicket) -> Result<&mut Effect, RunError> {
        if ticket.run != self.incarnation.run || ticket.owner != self.fence {
            return Err(invalid("foreign foreground effect ticket"));
        }
        self.effect_mut(ticket.sequence)
    }

    fn effect_mut(&mut self, sequence: i64) -> Result<&mut Effect, RunError> {
        let index = sequence
            .checked_sub(1)
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("invalid foreground effect sequence"))?;
        self.effects
            .get_mut(index)
            .ok_or_else(|| invalid("missing foreground effect"))
    }

    fn can_effect(&self) -> Result<(), RunError> {
        if matches!(self.phase, Phase::Stopping | Phase::Closed) {
            return Err(invalid("foreground run must settle before more effects"));
        }
        Ok(())
    }
}

fn invalid(message: &str) -> RunError {
    RunError::Invalid(message.into())
}

#[cfg(test)]
mod tests;
