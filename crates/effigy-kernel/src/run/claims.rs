//! Conflict sets are reserved, committed and released atomically. A pending
//! claim is never reclaimed from a missing heartbeat or an unknown holder.
use super::*;
use crate::ident;
use crate::observe::Observed;
use crate::store::{BootDeadline, Txn};
use rusqlite::OptionalExtension;
use std::time::Duration;

pub(super) const KIND: &str = "kernel:claims";
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Reservation {
    resources: Vec<ResourceId>,
    quarantine: Duration,
}
#[derive(Debug, Clone)]
pub struct ReservationToken {
    effect: EffectTicket,
}
#[derive(Debug)]
pub struct Claims;
impl Claims {
    /// The run's owner gate proves authority. All wanted resources and the
    /// journal intent commit together, before any engine effect.
    pub fn reserve(
        run: &mut Run<'_>,
        wanted: &[ResourceId],
        quarantine: Duration,
    ) -> Result<ReservationToken, RunError> {
        run.can_effect()?;
        run.cx
            .require(crate::ctx::Capability::AdapterEffect)
            .map_err(|_| RunError::Invalid("claims require Mutate".into()))?;
        if wanted.is_empty()
            || wanted.iter().any(|id| {
                !matches!(
                    id.kind(),
                    crate::id::ResourceKind::Data | crate::id::ResourceKind::Endpoint
                )
            })
        {
            return Err(RunError::Invalid(
                "claims require data or endpoint resources".into(),
            ));
        }
        let mut resources = wanted.to_vec();
        resources.sort_by_key(ResourceId::canonical);
        resources.dedup();
        let reservation = Reservation {
            resources,
            quarantine,
        };
        let clock = ident::boot_clock();
        let result = (|| {
            crate::fault_point!(ClaimReserveBefore)?;
            let sequence=run.store.transaction(&run.cx,|txn| {
                journal::check_fence(txn.conn(),&run.owner)?;
                for resource in &reservation.resources {
                    let row=txn.conn().query_row("SELECT state,quarantine_boot,quarantine_nanos FROM claims WHERE resource=?1",
                        [resource.name().to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<i64>>(2)?))).optional()?;
                    if let Some((state,boot,nanos))=row {
                        let expired=match (boot,nanos) {
                            (Some(boot),Some(nanos)) => matches!((BootDeadline { boot,nanos: nanos as u64 }).passed(&clock),Observed::Verified(true,_)),
                            _ => false,
                        };
                        if state!="released" && !(state=="quarantined" && expired) { return Err(RunError::Conflict(resource.clone())); }
                    }
                }
                let spec=EffectSpec { target:run.owner.clone(),kind:KIND.into(),payload:serde_json::to_value(&reservation)?,settlement:Settlement::Release(Value::Null),required:true };
                let sequence=journal::insert_effect(txn,run.id,&spec)?;
                let token=format!("{}:{sequence}",run.id);
                for resource in &reservation.resources {
                    txn.resource(resource,"claims")?;
                    // Claim generation belongs to the fenced owning run. The
                    // conflict row, not an independently held target gate, owns it.
                    txn.conn().execute("INSERT INTO claims(resource,holder,run_id,reservation_token,state,generation,revision) VALUES (?1,?2,?3,?4,'reserved',?5,0) \
                        ON CONFLICT(resource) DO UPDATE SET holder=excluded.holder,run_id=excluded.run_id,reservation_token=excluded.reservation_token,state='reserved',generation=excluded.generation,revision=0,quarantine_boot=NULL,quarantine_nanos=NULL",
                        params![resource.name().to_string(),run.owner.resource.name().to_string(),run.id.to_string(),token,run.owner.generation.get() as i64])?;
                }
                journal::write_effect(txn,run.id,sequence,EffectState::Applied,Some(&Value::String(token)),None)?;
                Ok::<_,RunError>(sequence)
            })?;
            crate::fault_point!(ClaimReserveAfter)?;
            Ok(ReservationToken {
                effect: EffectTicket {
                    run: run.id,
                    owner: run.owner.clone(),
                    sequence,
                },
            })
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }
    pub fn commit(
        run: &mut Run<'_>,
        token: &ReservationToken,
        revision: u64,
    ) -> Result<(), RunError> {
        run.can_effect()?;
        check_token(run, token)?;
        let revision = i64::try_from(revision)
            .map_err(|_| RunError::Invalid("claim revision overflow".into()))?;
        run.store.transaction(&run.cx,|txn| {
            journal::check_fence(txn.conn(),&run.owner)?;
            let effect=journal::effect(txn.conn(),run.id,token.effect.sequence)?;
            let reservation:Reservation=serde_json::from_value(effect.spec.payload)?;
            let name=format!("{}:{}",run.id,token.effect.sequence);
            for resource in reservation.resources {
                let changed=txn.conn().execute("UPDATE claims SET state='live',revision=?2 WHERE resource=?1 AND reservation_token=?3 AND state IN ('reserved','live')",
                    params![resource.name().to_string(),revision,name])?;
                if changed!=1 { return Err(RunError::Conflict(resource)); }
            }
            Ok::<_,RunError>(())
        })
    }
    /// Releases the entire reservation in one transaction with its journal
    /// result. Quarantine is measured from this commit's boot-clock sample.
    pub fn release(
        run: &mut Run<'_>,
        token: &ReservationToken,
        cleanup: &CleanupContext,
    ) -> Result<(), RunError> {
        check_token(run, token)?;
        if !matches!(run.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::Invalid("claim release requires settle".into()));
        }
        let effect = journal::effect(run.store.conn(), run.id, token.effect.sequence)?;
        run.settle_claims(&effect, &settle::cleanup_effects(cleanup)?)
    }
}
fn check_token(run: &Run<'_>, token: &ReservationToken) -> Result<(), RunError> {
    if token.effect.run != run.id || token.effect.owner != run.owner {
        return Err(RunError::Invalid("foreign reservation token".into()));
    }
    Ok(())
}
impl Run<'_> {
    pub(super) fn settle_claims(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        if effect.state == EffectState::Released {
            return Ok(());
        }
        let reservation: Reservation = serde_json::from_value(effect.spec.payload.clone())?;
        let clock = ident::boot_clock();
        self.store.transaction(cx, |txn| {
            // Claims protect surviving effects just as environment attachments do.
            if journal::effects(txn.conn(), self.id)?.iter().any(|e| {
                e.spec.kind != KIND && e.spec.kind != attachments::KIND && !e.state.terminal()
            }) {
                return journal::write_effect(
                    txn,
                    self.id,
                    effect.sequence,
                    EffectState::Residue,
                    None,
                    Some("unresolved effects retain claims"),
                );
            }
            let deadline = if reservation.quarantine.is_zero() {
                None
            } else {
                match &clock {
                    Observed::Verified(now, _) => {
                        Some(BootDeadline::after(now, reservation.quarantine))
                    }
                    _ => {
                        return journal::write_effect(
                            txn,
                            self.id,
                            effect.sequence,
                            EffectState::Residue,
                            None,
                            Some("boot clock unknown; claim retained"),
                        )
                    }
                }
            };
            crate::fault_point!(ClaimReleaseBefore)?;
            release_rows(
                txn,
                self.id,
                effect.sequence,
                &reservation,
                deadline.as_ref(),
            )?;
            journal::write_effect(
                txn,
                self.id,
                effect.sequence,
                EffectState::Released,
                None,
                None,
            )
        })?;
        crate::fault_point!(ClaimReleaseAfter)?;
        Ok(())
    }
}
fn release_rows(
    txn: &Txn<'_>,
    run: RunId,
    seq: i64,
    reservation: &Reservation,
    deadline: Option<&BootDeadline>,
) -> Result<(), RunError> {
    for resource in &reservation.resources {
        // Never touch a successor reservation. This exact-token condition also
        // makes a release retry after a crash a no-op (O5).
        txn.conn().execute("UPDATE claims SET state=?3,quarantine_boot=?4,quarantine_nanos=?5 WHERE resource=?1 AND reservation_token=?2 AND state IN ('reserved','live')",
            params![resource.name().to_string(),format!("{run}:{seq}"),if deadline.is_some(){"quarantined"}else{"released"},
                deadline.map(|d|&d.boot),deadline.map(|d|i64::try_from(d.nanos)).transpose().map_err(|_|RunError::Invalid("quarantine overflow".into()))?])?;
    }
    Ok(())
}
