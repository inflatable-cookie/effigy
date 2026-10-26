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
    #[serde(default)]
    retained: bool,
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
            .map_err(|_| RunError::refused("claims require Mutate"))?;
        if wanted.is_empty()
            || wanted.iter().any(|id| {
                !matches!(
                    id.kind(),
                    crate::id::ResourceKind::Data | crate::id::ResourceKind::Endpoint
                )
            })
        {
            return Err(RunError::refused(
                "claims require data or endpoint resources",
            ));
        }
        let mut resources = wanted.to_vec();
        resources.sort_by_key(ResourceId::canonical);
        resources.dedup();
        let reservation = Reservation {
            resources,
            quarantine,
            retained: false,
        };
        let clock = ident::boot_clock();
        let result = (|| {
            crate::fault_point!(ClaimReserveBefore)?;
            let sequence=run.store.transaction(&run.cx.child(std::time::Duration::from_secs(30)),|txn| {
                journal::check_fence(txn.conn(),&run.owner)?;
                for resource in &reservation.resources {
                    let row=txn.conn().query_row("SELECT state,quarantine_boot,quarantine_nanos FROM claims WHERE resource=?1",
                        [resource.name().to_string()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<i64>>(2)?))).optional()?;
                    if let Some((state,boot,nanos))=row {
                        let expired=match (boot,nanos) {
                            (Some(boot),Some(nanos)) => matches!((BootDeadline { boot,nanos: nanos as u64 }).passed(&clock),Observed::Verified(true,_)),
                            _ => false,
                        };
                        if state!="released" && !(state=="quarantined" && expired) { return Err(RunError::refused(format!("resource `{}` is already claimed by another run", resource.label()))); }
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
        let revision =
            i64::try_from(revision).map_err(|_| RunError::broken("claim revision overflow"))?;
        run.store.transaction(&run.cx.child(std::time::Duration::from_secs(30)),|txn| {
            journal::check_fence(txn.conn(),&run.owner)?;
            let effect=journal::effect(txn.conn(),run.id,token.effect.sequence)?;
            let reservation:Reservation=serde_json::from_value(effect.spec.payload)?;
            let name=format!("{}:{}",run.id,token.effect.sequence);
            for resource in reservation.resources {
                let changed=txn.conn().execute("UPDATE claims SET state='live',revision=?2 WHERE resource=?1 AND reservation_token=?3 AND state IN ('reserved','live')",
                    params![resource.name().to_string(),revision,name])?;
                if changed!=1 { return Err(RunError::refused(format!("resource `{}` is already claimed by another run", resource.label()))); }
            }
            Ok::<_,RunError>(())
        })
    }
    /// Atomically publishes an engine's complete persistent conflict set and
    /// opaque payload. New resources must already be reserved by `token`;
    /// existing resources must be retained by this exact owner resource.
    /// Missing engine inventory never grants ownership of a foreign claim.
    ///
    /// Publication records retention intent. Settle transfers new claims from
    /// the reserving run to its resource owner, so a later run can update or
    /// release them. Omitted claims are quarantined, never silently reclaimed.
    pub fn publish_retained(
        run: &mut Run<'_>,
        token: Option<&ReservationToken>,
        desired: &[(ResourceId, Value)],
        payload: &Value,
        quarantine: Duration,
    ) -> Result<(), RunError> {
        run.can_effect()?;
        run.cx
            .require(crate::ctx::Capability::AdapterEffect)
            .map_err(|_| RunError::refused("persistent claims require Mutate"))?;
        if let Some(token) = token {
            check_token(run, token)?;
        }
        let wanted = desired
            .iter()
            .map(|(id, _)| id.name().to_string())
            .collect::<std::collections::BTreeSet<_>>();
        if wanted.len() != desired.len()
            || desired
                .iter()
                .any(|(id, _)| id.kind() != crate::id::ResourceKind::Endpoint)
        {
            return Err(RunError::refused(
                "persistent conflict set requires unique endpoints",
            ));
        }
        let clock = ident::boot_clock();
        let result = (|| {
            crate::fault_point!(ClaimPublishBefore)?;
            run.store.transaction(&run.cx.child(Duration::from_secs(30)), |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                let owner = run.owner.resource.name().to_string();
                let revision = run.owner.generation.get() as i64;
                let reservation_name = token.map(|t| format!("{}:{}", run.id, t.effect.sequence));
                for (resource, details) in desired {
                    txn.resource(resource, "claims")?;
                    let row = txn.conn().query_row("SELECT holder,run_id,reservation_token,state FROM claims WHERE resource=?1",
                        [resource.name().to_string()], |r| Ok((r.get::<_,Option<String>>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,String>(3)?))).optional()?;
                    let valid = row.is_some_and(|(holder, id, name, state)| {
                        holder.as_deref() == Some(&owner) && (
                            (id.is_none() && state == "live") ||
                            (reservation_name.is_some() && name == reservation_name && id.as_deref() == Some(&run.id.to_string()) && matches!(state.as_str(), "reserved" | "live")))
                    });
                    if !valid { return Err(RunError::refused(format!("resource `{}` is already claimed by another run", resource.label()))); }
                    txn.conn().execute("UPDATE claims SET state='live',revision=?2,details=?3 WHERE resource=?1",
                        params![resource.name().to_string(),revision,serde_json::to_string(details)?])?;
                }
                let mut stmt = txn.conn().prepare("SELECT resource FROM claims WHERE holder=?1 AND (run_id IS NULL OR run_id=?2) AND state='live'")?;
                let omitted = stmt.query_map(params![owner,run.id.to_string()], |r| r.get::<_,String>(0))?.collect::<Result<Vec<_>,_>>()?;
                drop(stmt);
                for resource in omitted.into_iter().filter(|name| !wanted.contains(name)) {
                    let Observed::Verified(now, _) = &clock else {
                        return Err(RunError::unavailable("unknown quarantine clock"));
                    };
                    let deadline = BootDeadline::after(now, quarantine);
                    let nanos = i64::try_from(deadline.nanos).map_err(|_| RunError::broken("quarantine overflow"))?;
                    txn.conn().execute("UPDATE claims SET state='quarantined',run_id=NULL,revision=?2,quarantine_boot=?3,quarantine_nanos=?4 WHERE resource=?1",
                        params![resource,revision,deadline.boot,nanos])?;
                }
                if let Some(token) = token {
                    let mut effect = journal::effect(txn.conn(), run.id, token.effect.sequence)?;
                    let mut reservation: Reservation = serde_json::from_value(effect.spec.payload.clone())?;
                    if reservation.resources.iter().any(|r| !wanted.contains(&r.name().to_string())) {
                        return Err(RunError::broken("publication omits a reserved endpoint"));
                    }
                    reservation.retained = true;
                    effect.spec.payload = serde_json::to_value(reservation)?;
                    txn.conn().execute("UPDATE events SET target=?3 WHERE run_id=?1 AND seq=?2",
                        params![run.id.to_string(),token.effect.sequence,serde_json::to_string(&effect.spec)?])?;
                }
                txn.conn().execute("UPDATE resources SET state=?2,updated_at=?3 WHERE name=?1",
                    params![owner,serde_json::to_string(payload)?,txn.now()])?;
                // Existing reservations and releases also need a durable
                // publication event when there is no new reservation ticket.
                let publication = EffectSpec {
                    target: run.owner.clone(), kind: "kernel:claim-publication".into(),
                    payload: serde_json::json!({"revision":revision}),
                    settlement: Settlement::Retain, required: false,
                };
                let sequence = journal::insert_effect(txn, run.id, &publication)?;
                journal::write_effect(txn, run.id, sequence, EffectState::Retained, None, None)?;
                Ok::<_,RunError>(())
            })?;
            crate::fault_point!(ClaimPublishAfter)?;
            Ok(())
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
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
            return Err(RunError::broken("claim release requires settle"));
        }
        let effect = journal::effect(run.store.conn(), run.id, token.effect.sequence)?;
        run.settle_claims(&effect, &settle::cleanup_effects(cleanup)?)
    }
}
fn check_token(run: &Run<'_>, token: &ReservationToken) -> Result<(), RunError> {
    if token.effect.run != run.id || token.effect.owner != run.owner {
        return Err(RunError::broken("foreign reservation token"));
    }
    Ok(())
}
impl Run<'_> {
    pub(super) fn settle_claims(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        if effect.state.terminal() {
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
            if reservation.retained {
                crate::fault_point!(ClaimRetainBefore)?;
                for resource in &reservation.resources {
                    txn.conn().execute("UPDATE claims SET run_id=NULL WHERE resource=?1 AND reservation_token=?2 AND state='live'",
                        params![resource.name().to_string(),format!("{}:{}",self.id,effect.sequence)])?;
                }
                journal::write_effect(txn, self.id, effect.sequence, EffectState::Retained, None, None)?;
                crate::fault_point!(ClaimRetainAfter)?;
                return Ok(());
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
                deadline.map(|d|&d.boot),deadline.map(|d|i64::try_from(d.nanos)).transpose().map_err(|_|RunError::broken("quarantine overflow"))?])?;
    }
    Ok(())
}
