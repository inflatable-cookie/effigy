use super::*;
use crate::ctx::ObserveContext;
use crate::gate::OwnerState;
use crate::id::ResourceKind;
use crate::observe::Observed;
use crate::store::StoreReader;

pub(super) const KIND: &str = "kernel:attachment";
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum AttachMode {
    Borrow,
    Exclusive { stop_on_release: bool },
}
#[derive(Debug, Clone)]
pub struct AttachmentTicket {
    effect: EffectTicket,
    environment: ResourceId,
}
#[derive(Debug, Clone, Copy, Serialize)]
pub enum Liveness {
    Live,
    Interrupted,
}
#[derive(Debug, Clone, Serialize)]
pub struct Holder {
    pub run: RunId,
    pub liveness: Observed<Liveness>,
}
#[derive(Debug)]
pub struct Attachments;
/// Borrows the gated environment run exclusively. No caller can release the
/// gate or advance its generation while using the permit.
#[derive(Debug)]
pub struct TransitionPermit<'r, 'root> {
    run: &'r mut Run<'root>,
}
impl TransitionPermit<'_, '_> {
    pub fn intend(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        if spec.target != self.run.owner || spec.kind.starts_with("kernel:") {
            return Err(RunError::Invalid(
                "permit targets its environment generation only".into(),
            ));
        }
        self.run.can_effect()?;
        let result = (|| {
            crate::fault_point!(IntendBefore)?;
            let sequence = self.run.store.transaction(&self.run.cx, |txn| {
                journal::check_fence(txn.conn(), &self.run.owner)?;
                // Atomic with intent, even after a long adapter preparation.
                ensure_no_holders(txn.conn(), &self.run.owner.resource)?;
                journal::insert_effect(txn, self.run.id, &spec)
            })?;
            crate::fault_point!(IntendAfter)?;
            Ok(EffectTicket {
                run: self.run.id,
                owner: self.run.owner.clone(),
                sequence,
            })
        })();
        if result.is_err() {
            self.run.poisoned = true;
        }
        result
    }
    pub fn applied(&mut self, ticket: &EffectTicket, receipt: Value) -> Result<(), RunError> {
        self.run.applied(ticket, receipt)
    }
    pub fn completed(
        &mut self,
        ticket: &EffectTicket,
        obligation: crate::outcome::RequiredObligation,
    ) -> Result<(), RunError> {
        self.run.completed(ticket, obligation)
    }
    pub fn fence(&self) -> &Fence {
        &self.run.owner
    }
}
impl Attachments {
    /// Publishes an environment's applied generation under its owner gate.
    /// Engines call this only after their required startup effects completed.
    pub fn publish_active(run: &mut Run<'_>) -> Result<(), RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env
            || run
                .effects()?
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(RunError::Invalid("environment is not ready".into()));
        }
        run.store.transaction(&run.cx, |txn| {
            journal::check_fence(txn.conn(), &run.owner)?;
            txn.conn().execute(
                "UPDATE resources SET state='active',last_applied=?2,updated_at=?3 WHERE name=?1",
                params![
                    run.owner.resource.name().to_string(),
                    run.owner.generation.get().to_string(),
                    txn.now()
                ],
            )?;
            Ok::<_, RunError>(())
        })
    }
    pub fn attach(
        run: &mut Run<'_>,
        env: &Fence,
        mode: AttachMode,
        wait: Wait,
    ) -> Result<AttachmentTicket, RunError> {
        run.can_effect()?;
        if env.resource.kind() != ResourceKind::Env || env.resource == run.owner.resource {
            return Err(RunError::Invalid(
                "attachment needs a distinct environment".into(),
            ));
        }
        if run.owner.resource.canonical() >= env.resource.canonical() {
            return Err(RunError::Invalid(
                "attachment would reverse canonical gate order".into(),
            ));
        }
        let _gate = Gate::acquire(&run.cx, run.root, std::slice::from_ref(&env.resource), wait)?;
        let result = (|| {
            crate::fault_point!(AttachBefore)?;
            let sequence = run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(),&run.owner)?;
                journal::check_fence(txn.conn(),env)?;
                let active: bool = txn.conn().query_row("SELECT state='active' AND last_applied=cast(generation AS TEXT) FROM resources WHERE name=?1",
                    [env.resource.name().to_string()],|r| r.get::<_,Option<bool>>(0))?.unwrap_or(false);
                if !active { return Err(RunError::EnvNotActive); }
                let spec = EffectSpec { target:env.clone(),kind:KIND.into(),payload:serde_json::to_value(mode)?,settlement:Settlement::Reconcile(Value::Null),required:true };
                let sequence=journal::insert_effect(txn,run.id,&spec)?;
                txn.conn().execute("INSERT INTO attachments(environment,run_id,env_generation,mode,state) VALUES (?1,?2,?3,?4,'attached')",
                    params![env.resource.name().to_string(),run.id.to_string(),env.generation.get() as i64,serde_json::to_string(&mode)?])?;
                journal::write_effect(txn,run.id,sequence,EffectState::Applied,Some(&Value::Bool(true)),None)?;
                Ok::<_,RunError>(sequence)
            })?;
            crate::fault_point!(AttachAfter)?;
            Ok(AttachmentTicket {
                effect: EffectTicket {
                    run: run.id,
                    owner: run.owner.clone(),
                    sequence,
                },
                environment: env.resource.clone(),
            })
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }
    /// Only the holder's settle releases protection. No environment gate is
    /// acquired, so a waiting transition and its holder cannot deadlock.
    pub fn release(
        run: &mut Run<'_>,
        ticket: &AttachmentTicket,
        cleanup: &CleanupContext,
    ) -> Result<(), RunError> {
        if ticket.effect.run != run.id || ticket.effect.owner != run.owner {
            return Err(RunError::Invalid("foreign attachment".into()));
        }
        let effect = journal::effect(run.store.conn(), run.id, ticket.effect.sequence)?;
        if effect.spec.target.resource != ticket.environment || effect.spec.kind != KIND {
            return Err(RunError::Invalid("foreign attachment effect".into()));
        }
        if !matches!(run.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::Invalid(
                "attachment release requires settle".into(),
            ));
        }
        run.settle_attachment(&effect, &settle::cleanup_effects(cleanup)?)
    }
    pub fn permit<'r, 'root>(
        run: &'r mut Run<'root>,
    ) -> Result<TransitionPermit<'r, 'root>, RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env {
            return Err(RunError::Invalid("permit needs environment owner".into()));
        }
        journal::check_fence(run.store.conn(), &run.owner)?;
        ensure_no_holders(run.store.conn(), &run.owner.resource)?;
        Ok(TransitionPermit { run })
    }
    /// Failed reads remain Unknown; a free holder gate means interrupted, never
    /// absent. Only the atomic release row removes holder protection.
    pub fn holders(
        cx: &ObserveContext,
        root: &StateRoot,
        reader: &StoreReader,
        env: &ResourceId,
    ) -> Observed<Vec<Holder>> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        match attached(reader.conn(), env) {
            Ok(ids) => Observed::Verified(
                ids.into_iter()
                    .map(|id| {
                        let liveness = match reader.run_owner_state(cx, root, &id) {
                            Observed::Verified(OwnerState::Held, _) => {
                                Observed::Verified(Liveness::Live, settle::evidence())
                            }
                            Observed::Verified(OwnerState::Interrupted, _) => {
                                Observed::Verified(Liveness::Interrupted, settle::evidence())
                            }
                            _ => Observed::Unknown(UnknownReason::Inconsistent(
                                "attachment not positively released".into(),
                            )),
                        };
                        Holder { run: id, liveness }
                    })
                    .collect(),
                settle::evidence(),
            ),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }
}
fn attached(conn: &rusqlite::Connection, env: &ResourceId) -> Result<Vec<RunId>, RunError> {
    let mut stmt=conn.prepare("SELECT r.run_id FROM attachments a JOIN runs r ON r.run_id=a.run_id WHERE a.environment=?1 AND a.state<>'released'")?;
    let ids = stmt
        .query_map([env.name().to_string()], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    ids.into_iter()
        .map(|id| {
            id.parse()
                .map_err(|e: crate::id::IdentityError| RunError::Store(e.into()))
        })
        .collect()
}
pub(super) fn ensure_no_holders(
    conn: &rusqlite::Connection,
    env: &ResourceId,
) -> Result<(), RunError> {
    let ids = attached(conn, env)?;
    // Even an interrupted holder protects until its own gated settle releases.
    // No liveness guess can turn an attachment into permission (R3/R4).
    if !ids.is_empty() {
        return Err(RunError::HeldBy(
            ids.into_iter()
                .map(|run| Holder {
                    run,
                    liveness: Observed::Unknown(UnknownReason::Inconsistent(
                        "holder must settle".into(),
                    )),
                })
                .collect(),
        ));
    }
    Ok(())
}
impl Run<'_> {
    pub(super) fn settle_attachment(
        &mut self,
        effect: &Effect,
        cx: &EffectContext,
    ) -> Result<(), RunError> {
        if effect.state == EffectState::Released {
            return Ok(());
        }
        self.store.transaction(cx,|txn| {
            let blocked=journal::effects(txn.conn(),self.id)?.iter().any(|e|e.spec.kind!=KIND && !e.state.terminal());
            if blocked { return journal::write_effect(txn,self.id,effect.sequence,EffectState::Residue,None,Some("unresolved effects retain holder protection")); }
            crate::fault_point!(AttachmentReleaseBefore)?;
            txn.conn().execute("UPDATE attachments SET state='released' WHERE environment=?1 AND run_id=?2 AND state<>'released'",
                params![effect.spec.target.resource.name().to_string(),self.id.to_string()])?;
            journal::write_effect(txn,self.id,effect.sequence,EffectState::Released,None,None)?;
            Ok::<_,RunError>(())
        })?;
        crate::fault_point!(AttachmentReleaseAfter)?;
        Ok(())
    }
}
