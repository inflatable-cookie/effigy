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
/// The fixed additive publication obligation; never authorizes a stop, reset,
/// replacement of an existing immutable object, or a change to a live client.
pub const GUEST_PUBLICATION_KIND: &str = "kernel:workspace-guest-publication";

/// Only the kernel can construct a ticket for this additive operation.
#[derive(Debug)]
pub struct GuestPublicationTicket(EffectTicket);

/// Verified publication facts recorded before the environment becomes active.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuestPublicationReceipt {
    pub environment: Fence,
    pub container_id: String,
    pub vm_boot_id: String,
    pub artifact_digest: String,
    pub context_generation: u64,
}

fn check_guest_receipt(run: &Run<'_>, receipt: &GuestPublicationReceipt) -> Result<(), RunError> {
    let hex = receipt
        .artifact_digest
        .strip_prefix("sha256:")
        .unwrap_or("");
    if receipt.environment != run.owner
        || receipt.container_id.len() != 64
        || !receipt.container_id.bytes().all(|b| b.is_ascii_hexdigit())
        || receipt.vm_boot_id.trim().is_empty()
        || hex.len() != 64
        || !hex.bytes().all(|b| b.is_ascii_hexdigit())
        || receipt.context_generation == 0
    {
        return Err(RunError::refused(
            "invalid workspace guest publication receipt",
        ));
    }
    Ok(())
}

#[derive(Debug)]
pub struct Attachments;

/// The one `active` publication every environment activation writes: the
/// resource is active at its own run's generation, and `last_applied` names
/// that generation so a holder's `attach` can prove it.
pub(crate) fn publish_active_state(
    txn: &mut crate::store::Txn<'_>,
    owner: &Fence,
) -> Result<(), RunError> {
    txn.conn().execute(
        "UPDATE resources SET state='active',last_applied=?2,updated_at=?3 WHERE name=?1",
        params![
            owner.resource.name().to_string(),
            owner.generation.get().to_string(),
            txn.now()
        ],
    )?;
    Ok(())
}

/// The activation commit's fence check, so the reservation seam verifies the
/// environment generation with the kernel's own rule rather than a copy.
pub(crate) fn check_activation_fence(
    conn: &rusqlite::Connection,
    owner: &Fence,
) -> Result<(), RunError> {
    journal::check_fence(conn, owner)
}

/// One holder's `Borrow` attachment, written as an `Applied` effect of that
/// holder's own journal in the caller's transaction. The environment activation
/// run is the only writer: `publish_active_for` and the workspace activation
/// commit both install a holder this way, so the holder's settle releases it by
/// exactly the rule it would have used had it attached itself.
pub(crate) fn holder_attachment(
    txn: &mut crate::store::Txn<'_>,
    env: &Fence,
    holder: RunId,
    mode: AttachMode,
) -> Result<(), RunError> {
    let record = journal::run(txn.conn(), holder)?;
    let owner: Vec<u8> = txn.conn().query_row(
        "SELECT canonical FROM resources WHERE name=?1",
        [&record.owner],
        |r| r.get(0),
    )?;
    if matches!(record.phase, Phase::Stopping | Phase::Closed) || owner >= env.resource.canonical()
    {
        return Err(RunError::refused(
            "requesting holder is settling or out of gate order",
        ));
    }
    let spec = EffectSpec {
        target: env.clone(),
        kind: KIND.into(),
        payload: serde_json::to_value(mode)?,
        settlement: Settlement::Reconcile(Value::Null),
        required: true,
    };
    let sequence = journal::insert_effect(txn, holder, &spec)?;
    txn.conn().execute(
        "INSERT INTO attachments(environment,run_id,env_generation,mode,state) VALUES (?1,?2,?3,?4,'attached') \
         ON CONFLICT(environment,run_id) DO UPDATE SET env_generation=excluded.env_generation,mode=excluded.mode,state='attached'",
        params![
            env.resource.name().to_string(),
            holder.to_string(),
            env.generation.get() as i64,
            serde_json::to_string(&mode)?
        ],
    )?;
    journal::write_effect(
        txn,
        holder,
        sequence,
        EffectState::Applied,
        Some(&Value::Bool(true)),
        None,
    )
}
/// Borrows the gated environment run exclusively. No caller can release the
/// gate or advance its generation while using the permit.
#[derive(Debug)]
pub struct TransitionPermit<'r, 'root> {
    run: &'r mut Run<'root>,
    /// The one holder allowed to remain: a run settling itself (R4).
    exempt: Option<RunId>,
}
impl TransitionPermit<'_, '_> {
    pub fn intend(&mut self, spec: EffectSpec) -> Result<EffectTicket, RunError> {
        if spec.target != self.run.owner || spec.kind.starts_with("kernel:") {
            return Err(RunError::refused(
                "permit targets its environment generation only",
            ));
        }
        self.run.can_effect()?;
        let result = (|| {
            crate::fault_point!(IntendBefore)?;
            let sequence = self.run.store.transaction(&self.run.cx, |txn| {
                journal::check_fence(txn.conn(), &self.run.owner)?;
                // Atomic with intent, even after a long adapter preparation.
                ensure_no_holders_except(txn.conn(), &self.run.owner.resource, self.exempt)?;
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
    /// Publishes that the environment's workloads were stopped. The state
    /// change re-checks holders in its own transaction, so no attachment can
    /// observe `active` for an environment a transition just stopped.
    pub fn publish_stopped(&mut self) -> Result<(), RunError> {
        self.run.can_effect()?;
        let result = (|| {
            crate::fault_point!(EnvStateBefore)?;
            self.run.store.transaction(
                &self.run.cx.child(std::time::Duration::from_secs(30)),
                |txn| {
                    journal::check_fence(txn.conn(), &self.run.owner)?;
                    ensure_no_holders_except(txn.conn(), &self.run.owner.resource, self.exempt)?;
                    txn.conn().execute(
                        "UPDATE resources SET state='stopped',updated_at=?2 WHERE name=?1",
                        params![self.run.owner.resource.name().to_string(), txn.now()],
                    )?;
                    Ok::<_, RunError>(())
                },
            )?;
            crate::fault_point!(EnvStateAfter)?;
            Ok(())
        })();
        if result.is_err() {
            self.run.poisoned = true;
        }
        result
    }
}
impl Attachments {
    /// Journal only additive immutable guest publication under Gate(Env).
    /// Existing holders are preserved: this operation never reconfigures their
    /// runtime or rewrites the executable/context they already selected.
    pub fn intend_guest_publication(run: &mut Run<'_>) -> Result<GuestPublicationTicket, RunError> {
        if run.owner.resource.kind() != ResourceKind::Env {
            return Err(RunError::refused(
                "guest publication needs the environment owner",
            ));
        }
        run.can_effect()?;
        let spec = EffectSpec {
            target: run.owner.clone(),
            kind: GUEST_PUBLICATION_KIND.to_owned(),
            payload: serde_json::json!({"schema": "effigy.workspace-guest-publication.v1"}),
            settlement: Settlement::Retain,
            required: true,
        };
        let result = (|| {
            crate::fault_point!(IntendBefore)?;
            let sequence = run.store.transaction(&run.cx, |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                let sequence = journal::insert_effect(txn, run.id, &spec)?;
                // An earlier active state must not advertise this unfinished
                // activation after a partial publication or lost receipt.
                txn.conn().execute(
                    "UPDATE resources SET state='pending',updated_at=?2 WHERE name=?1",
                    params![run.owner.resource.name().to_string(), txn.now()],
                )?;
                Ok::<_, RunError>(sequence)
            })?;
            crate::fault_point!(IntendAfter)?;
            Ok(GuestPublicationTicket(EffectTicket {
                run: run.id,
                owner: run.owner.clone(),
                sequence,
            }))
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }

    /// Record the exact identity and digest, then complete the required
    /// obligation. No active publication may precede this receipt.
    pub fn guest_published(
        run: &mut Run<'_>,
        ticket: &GuestPublicationTicket,
        receipt: GuestPublicationReceipt,
    ) -> Result<(), RunError> {
        let reserved: bool = run.store.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_activation_reservations WHERE environment_run=?1)",
            [run.id.to_string()], |row| row.get(0),
        )?;
        if reserved {
            return Err(RunError::refused(
                "a workspace activation requires its session-bound publication receipt",
            ));
        }
        check_guest_receipt(run, &receipt)?;
        run.applied(&ticket.0, serde_json::to_value(receipt)?)?;
        run.completed(&ticket.0, crate::outcome::RequiredObligation::Applied)
    }

    /// Record the guest's acknowledgement of the exact frozen workspace
    /// session, before its atomic activation commit. The generic publication
    /// receipt alone cannot authenticate a workspace session mapping.
    pub fn workspace_guest_published(
        run: &mut Run<'_>,
        ticket: &GuestPublicationTicket,
        receipt: crate::workspace_session::WorkspacePublicationReceipt,
    ) -> Result<(), RunError> {
        check_guest_receipt(run, &receipt.publication)?;
        if receipt.activation_run != run.id {
            return Err(RunError::refused(
                "workspace publication names another activation run",
            ));
        }
        let binding = &receipt.session;
        let matches: bool = run.store.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_activation_reservations \
             WHERE environment=?1 AND environment_generation=?2 AND environment_run=?3 AND state='reserved' \
             AND session_owner=?4 AND session_run=?5 AND session_generation=?6)",
            params![run.owner.resource.name().to_string(), run.owner.generation.get() as i64,
                run.id.to_string(), binding.owner.name().to_string(), binding.run.to_string(), binding.generation.get() as i64],
            |row| row.get(0),
        )?;
        if !matches {
            return Err(RunError::from(
                crate::workspace_session::ActivationRefusal::ReceiptMismatch,
            ));
        }
        run.applied(&ticket.0, serde_json::to_value(receipt)?)?;
        run.completed(&ticket.0, crate::outcome::RequiredObligation::Applied)
    }

    /// Record that the guest holds exactly the selection this obligation's
    /// commit wrote, under the environment run's own held `Gate(Env)`.
    ///
    /// The caller supplies only a reader of the exact target: the kernel reads
    /// every selector and every selected object back itself and compares them
    /// with the committed targets, so a partial switch, a third value or a
    /// missing object is refused and a receipt the caller assembled is never
    /// accepted. It also refuses a generation that no longer serves this
    /// obligation and a second completion: a selection completes or abandons
    /// exactly once. The retained launch pins the successor owner gate through
    /// completion and revalidates its bound listener after readback. No
    /// transaction spans guest I/O.
    pub fn selection_switched(
        run: &mut Run<'_>,
        obligation: &crate::workspace_session::SelectionObligation,
        launch: &crate::workspace_session::SessionLaunch,
        reader: &mut dyn crate::workspace_session::SelectorReader,
    ) -> Result<(), RunError> {
        use crate::workspace_session::SelectionState;
        run.can_effect()?;
        if run.id != obligation.run()
            || run.owner != obligation.fence()
            || run.owner.resource != *obligation.environment()
        {
            return Err(crate::workspace_session::ActivationRefusal::SelectionMismatch.into());
        }
        let receipt = obligation.observe(reader)?;
        obligation.validate_switch(&receipt)?;
        let custody = launch.hold_lease(obligation.receipt().serving())?;
        let result = (|| {
            crate::fault_point!(SelectionSwitchBefore)?;
            run.store
                .transaction(&run.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_fence(txn.conn(), &run.owner)?;
                    obligation.require_serving(txn)?;
                    custody.revalidate(obligation.receipt().serving())?;
                    obligation.finish(txn, SelectionState::Switched, None)
                })?;
            crate::fault_point!(SelectionSwitchAfter)?;
            Ok(())
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
    }

    /// An attempted publication with no trustworthy receipt is uncertain;
    /// recovery retains additive bytes and never replays a container effect.
    pub fn guest_publication_unknown(
        run: &mut Run<'_>,
        ticket: &GuestPublicationTicket,
        reason: UnknownReason,
    ) -> Result<(), RunError> {
        run.unknown(&ticket.0, reason)
    }

    /// Publishes an environment's applied generation under its owner gate.
    /// A still-mapped workspace session retains its applied fence through a
    /// no-op publication; changing that fence requires its atomic successor commit.
    /// Engines call this only after their required startup effects completed.
    pub fn publish_active(run: &mut Run<'_>) -> Result<(), RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env
            || run
                .effects()?
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(RunError::refused("environment is not ready"));
        }
        run.store
            .transaction(&run.cx.child(std::time::Duration::from_secs(30)), |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                crate::workspace_session::refuse_reserved_generation(
                    txn.conn(),
                    &run.owner.resource,
                    run.owner.generation,
                )?;
                if crate::workspace_session::preserve_active_predecessor(txn, &run.owner)?.is_some()
                {
                    return Ok(());
                }
                publish_active_state(txn, &run.owner)
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
            return Err(RunError::refused("attachment needs a distinct environment"));
        }
        if run.owner.resource.canonical() >= env.resource.canonical() {
            return Err(RunError::refused(
                "attachment would reverse canonical gate order",
            ));
        }
        if let crate::id::Scope::Checkout(checkout) = env.resource.scope() {
            run.protect_checkout(checkout, wait)?;
        }
        let _gate = Gate::acquire(&run.cx, run.root, std::slice::from_ref(&env.resource), wait)?;
        let result = (|| {
            crate::fault_point!(AttachBefore)?;
            let sequence = run.store.transaction(&run.cx.child(std::time::Duration::from_secs(30)), |txn| {
                journal::check_fence(txn.conn(), &run.owner)?;
                let preserved = matches!(mode, AttachMode::Borrow)
                    && crate::workspace_session::preserved_environment(txn.conn(), env)?;
                if !preserved {
                    journal::check_fence(txn.conn(),env)?;
                    let active: bool = txn.conn().query_row("SELECT state='active' AND last_applied=cast(generation AS TEXT) FROM resources WHERE name=?1",
                        [env.resource.name().to_string()],|r| r.get::<_,Option<bool>>(0))?.unwrap_or(false);
                    if !active { return Err(RunError::EnvNotActive); }
                }
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
            return Err(RunError::broken("foreign attachment"));
        }
        let effect = journal::effect(run.store.conn(), run.id, ticket.effect.sequence)?;
        if effect.spec.target.resource != ticket.environment || effect.spec.kind != KIND {
            return Err(RunError::broken("foreign attachment effect"));
        }
        if !matches!(run.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::broken("attachment release requires settle"));
        }
        run.settle_attachment(&effect, &settle::cleanup_effects(cleanup)?)
    }
    pub fn permit<'r, 'root>(
        run: &'r mut Run<'root>,
    ) -> Result<TransitionPermit<'r, 'root>, RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env {
            return Err(RunError::broken("permit needs environment owner"));
        }
        journal::check_fence(run.store.conn(), &run.owner)?;
        ensure_no_holders(run.store.conn(), &run.owner.resource)?;
        Ok(TransitionPermit { run, exempt: None })
    }
    /// The permit for a holder's own stop (R4): issued when the only holder
    /// left is `exempt` and that run has already entered its settle. Its
    /// attachment keeps protecting against every other transition.
    pub fn permit_exempt<'r, 'root>(
        run: &'r mut Run<'root>,
        exempt: RunId,
    ) -> Result<TransitionPermit<'r, 'root>, RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env {
            return Err(RunError::broken("permit needs environment owner"));
        }
        journal::check_fence(run.store.conn(), &run.owner)?;
        ensure_no_holders_except(run.store.conn(), &run.owner.resource, Some(exempt))?;
        Ok(TransitionPermit {
            run,
            exempt: Some(exempt),
        })
    }
    /// The ensure path for a holder that found its environment inactive
    /// (§5.9 `EnvNotActive`): the environment's apply run publishes `active`
    /// and inserts each requesting run's attachment in the same transaction,
    /// as an `Applied` effect of that holder's own journal. The holder's
    /// settle releases it exactly as if it had attached itself.
    /// A live mapped workspace retains its applied fence; new holders can only
    /// Borrow that fence until an atomic workspace activation replaces it.
    pub fn publish_active_for(
        run: &mut Run<'_>,
        holders: &[(RunId, AttachMode)],
    ) -> Result<(), RunError> {
        run.can_effect()?;
        if run.owner.resource.kind() != ResourceKind::Env
            || run
                .effects()?
                .iter()
                .any(|e| e.spec.required && e.state != EffectState::Applied)
        {
            return Err(RunError::refused("environment is not ready"));
        }
        let result = (|| {
            crate::fault_point!(AttachBefore)?;
            run.store
                .transaction(&run.cx.child(std::time::Duration::from_secs(30)), |txn| {
                    journal::check_fence(txn.conn(), &run.owner)?;
                    crate::workspace_session::refuse_reserved_generation(
                        txn.conn(), &run.owner.resource, run.owner.generation,
                    )?;
                    let applied = match crate::workspace_session::preserve_active_predecessor(txn, &run.owner)? {
                        Some(applied) => applied,
                        None => {
                            publish_active_state(txn, &run.owner)?;
                            run.owner.clone()
                        }
                    };
                    for (holder, mode) in holders {
                        let exists: bool = txn.conn().query_row(
                            "SELECT EXISTS(SELECT 1 FROM attachments WHERE environment=?1 AND run_id=?2 AND state<>'released')",
                            params![run.owner.resource.name().to_string(), holder.to_string()],
                            |r| r.get(0),
                        )?;
                        if exists {
                            continue;
                        }
                        if applied != run.owner && !matches!(mode, AttachMode::Borrow) {
                            return Err(RunError::refused("a preserved workspace fence permits only Borrow holders"));
                        }
                        holder_attachment(txn, &applied, *holder, *mode)?;
                    }
                    Ok::<_, RunError>(())
                })?;
            crate::fault_point!(AttachAfter)?;
            Ok(())
        })();
        if result.is_err() {
            run.poisoned = true;
        }
        result
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
    ensure_no_holders_except(conn, env, None)
}
/// An exempt holder is skipped only once its own run has entered settle.
fn ensure_no_holders_except(
    conn: &rusqlite::Connection,
    env: &ResourceId,
    exempt: Option<RunId>,
) -> Result<(), RunError> {
    let mut ids = attached(conn, env)?;
    if let Some(exempt) = exempt {
        let settling = matches!(
            journal::run(conn, exempt).map(|record| record.phase),
            Ok(Phase::Stopping | Phase::Closed)
        );
        if settling {
            ids.retain(|id| *id != exempt);
        }
    }
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

// A binding is a journal fact, not guest I/O. Insertion and its Applied receipt
// are atomic, so a crash exposes either no binding or the entire binding.
const WORKSPACE_TARGET_KIND: &str = "kernel:workspace-target";

impl Run<'_> {
    /// Read the uniquely journalled target under this exact claimed Env run.
    pub fn workspace_bound_target(
        &self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
    ) -> Result<crate::workspace_session::ContainerIdentity, RunError> {
        self.verify_workspace_reservation(reservation)?;
        workspace_fact(self.store.conn(), self.id, WORKSPACE_TARGET_KIND)
    }

    pub(crate) fn workspace_bound_platform(
        &self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
    ) -> Result<crate::workspace_session::BoundPlatform, RunError> {
        self.verify_workspace_reservation(reservation)?;
        workspace_fact(self.store.conn(), self.id, WORKSPACE_PLATFORM_KIND)
    }

    pub(crate) fn bind_workspace_target(
        &mut self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
        candidates: Observed<Vec<crate::workspace_session::TargetCandidate>>,
    ) -> Result<crate::workspace_session::ContainerIdentity, RunError> {
        use crate::workspace_session::{ActivationRefusal, ContainerIdentity, TargetEvidence};
        self.can_effect()?;
        let candidate = match candidates {
            Observed::Verified(mut candidates, _) if candidates.len() == 1 => candidates.remove(0),
            Observed::Unknown(reason) => return Err(RunError::unavailable(reason.to_string())),
            _ => return Err(ActivationRefusal::TargetAmbiguous.into()),
        };
        let target = &candidate.identity;
        if target.container_id.len() != 64
            || !target.container_id.bytes().all(|b| b.is_ascii_hexdigit())
            || target.vm_boot_id.trim().is_empty()
            || target.backend.trim().is_empty()
        {
            return Err(ActivationRefusal::ReceiptIncomplete.into());
        }
        crate::fault_point!(IntendBefore)?;
        self.store
            .transaction(&self.cx, |txn| {
                journal::check_fence(txn.conn(), &self.owner)?;
                let inputs = reservation.claimed_inputs(txn.conn(), self.id, &self.owner)?;
                match inputs.target {
                    TargetEvidence::Observed(ref identity) if identity == target => {}
                    TargetEvidence::Absent {
                        ref project,
                        ref backend,
                    } if project == &candidate.project
                        && backend == &target.backend
                        && candidate.created_by == Some(self.id) => {}
                    _ => return Err(ActivationRefusal::TargetEvidenceMismatch.into()),
                }
                let existing = journal::effects(txn.conn(), self.id)?
                    .into_iter()
                    .find(|effect| effect.spec.kind == WORKSPACE_TARGET_KIND);
                if let Some(existing) = existing {
                    let bound: ContainerIdentity = serde_json::from_value(existing.spec.payload)?;
                    if &bound != target || existing.state != EffectState::Applied {
                        return Err(ActivationRefusal::TargetEvidenceMismatch.into());
                    }
                    return Ok(bound);
                }
                let value = serde_json::to_value(target)?;
                let spec = EffectSpec {
                    target: self.owner.clone(),
                    kind: WORKSPACE_TARGET_KIND.to_owned(),
                    payload: value.clone(),
                    settlement: Settlement::Retain,
                    // This is an atomic fact, not a workload completion obligation.
                    required: false,
                };
                let sequence = journal::insert_effect(txn, self.id, &spec)?;
                journal::write_effect(
                    txn,
                    self.id,
                    sequence,
                    EffectState::Applied,
                    Some(&value),
                    None,
                )?;
                Ok(target.clone())
            })
            .and_then(|bound| {
                crate::fault_point!(IntendAfter)?;
                Ok(bound)
            })
    }
}

const WORKSPACE_PLATFORM_KIND: &str = "kernel:workspace-platform";
pub const GUEST_BINARY_KIND: &str = "kernel:workspace-guest-binary";

/// A ticket issued only after the required additive stage intent is durable.
#[derive(Debug)]
pub struct GuestBinaryTicket(EffectTicket);

fn workspace_fact<T: serde::de::DeserializeOwned>(
    conn: &rusqlite::Connection,
    run: RunId,
    kind: &str,
) -> Result<T, RunError> {
    let facts: Vec<_> = journal::effects(conn, run)?
        .into_iter()
        .filter(|effect| effect.spec.kind == kind)
        .collect();
    if facts.len() != 1 || facts[0].state != EffectState::Applied {
        return Err(RunError::refused(
            "workspace binding is absent or uncertain",
        ));
    }
    Ok(serde_json::from_value(facts[0].spec.payload.clone())?)
}

impl Run<'_> {
    pub(crate) fn bind_workspace_platform(
        &mut self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
        observed: Observed<crate::workspace_session::BoundPlatform>,
    ) -> Result<crate::workspace_session::BoundPlatform, RunError> {
        use crate::workspace_session::{BoundPlatform, ContainerIdentity};
        self.can_effect()?;
        let bound = match observed {
            Observed::Verified(bound, _) => bound,
            Observed::Unknown(reason) => return Err(RunError::unavailable(reason.to_string())),
            Observed::Absent(_) => return Err(RunError::refused("image facts absent")),
        };
        crate::fault_point!(IntendBefore)?;
        self.store
            .transaction(&self.cx, |txn| {
                journal::check_fence(txn.conn(), &self.owner)?;
                let inputs = reservation.claimed_inputs(txn.conn(), self.id, &self.owner)?;
                let target: ContainerIdentity =
                    workspace_fact(txn.conn(), self.id, WORKSPACE_TARGET_KIND)?;
                if target != bound.target {
                    return Err(
                        crate::workspace_session::ActivationRefusal::TargetEvidenceMismatch.into(),
                    );
                }
                bound.validate(&inputs)?;
                if journal::effects(txn.conn(), self.id)?
                    .iter()
                    .any(|effect| effect.spec.kind == WORKSPACE_PLATFORM_KIND)
                {
                    let existing: BoundPlatform =
                        workspace_fact(txn.conn(), self.id, WORKSPACE_PLATFORM_KIND)?;
                    if existing != bound {
                        return Err(RunError::refused("platform binding is immutable"));
                    }
                    return Ok(existing);
                }
                let value = serde_json::to_value(&bound)?;
                let sequence = journal::insert_effect(
                    txn,
                    self.id,
                    &EffectSpec {
                        target: self.owner.clone(),
                        kind: WORKSPACE_PLATFORM_KIND.into(),
                        payload: value.clone(),
                        settlement: Settlement::Retain,
                        required: false,
                    },
                )?;
                journal::write_effect(
                    txn,
                    self.id,
                    sequence,
                    EffectState::Applied,
                    Some(&value),
                    None,
                )?;
                Ok(bound.clone())
            })
            .and_then(|bound| {
                crate::fault_point!(IntendAfter)?;
                Ok(bound)
            })
    }

    pub(crate) fn workspace_binary_stage(
        &self,
        reservation: &crate::workspace_session::WorkspaceActivationReservation,
    ) -> Result<crate::workspace_session::GuestBinaryStage, RunError> {
        self.can_effect()?;
        journal::check_fence(self.store.conn(), &self.owner)?;
        reservation.claimed_inputs(self.store.conn(), self.id, &self.owner)?;
        Ok(crate::workspace_session::GuestBinaryStage {
            environment: self.owner.clone(),
            run: self.id,
            reservation: reservation.id(),
            binding: workspace_fact(self.store.conn(), self.id, WORKSPACE_PLATFORM_KIND)?,
        })
    }

    /// The exact staged object is mandatory for activation, even when a caller
    /// omitted the intent entirely. Required-effect scanning alone cannot prove
    /// that an obligatory effect was ever recorded.
    pub(crate) fn verified_workspace_binary(
        &self,
    ) -> Result<crate::workspace_session::GuestBinaryStage, RunError> {
        let stage: crate::workspace_session::GuestBinaryStage =
            workspace_fact(self.store.conn(), self.id, GUEST_BINARY_KIND)?;
        let effect = journal::effects(self.store.conn(), self.id)?
            .into_iter()
            .find(|effect| effect.spec.kind == GUEST_BINARY_KIND)
            .ok_or_else(|| RunError::refused("binary stage is absent"))?;
        if !effect.spec.required || effect.spec.target != self.owner {
            return Err(RunError::refused(
                "binary stage obligation is not required at this fence",
            ));
        }
        let receipt: crate::workspace_session::GuestBinaryReceipt = serde_json::from_value(
            effect
                .receipt
                .ok_or_else(|| RunError::refused("binary stage has no verified receipt"))?,
        )?;
        if receipt.target != stage.binding.target
            || receipt.platform != stage.binding.platform
            || receipt.object != stage.binding.artifact
        {
            return Err(RunError::refused(
                "binary stage receipt differs from its intent",
            ));
        }
        let binding: crate::workspace_session::BoundPlatform =
            workspace_fact(self.store.conn(), self.id, WORKSPACE_PLATFORM_KIND)?;
        if stage.environment != self.owner || stage.run != self.id || stage.binding != binding {
            return Err(RunError::refused("workspace binary stage binding differs"));
        }
        Ok(stage)
    }
}

impl Attachments {
    /// Record a required no-replace stage before any guest write. A repeat
    /// returns the same ticket; it never authorizes another object's write.
    pub fn intend_guest_binary(
        run: &mut Run<'_>,
        stage: &crate::workspace_session::GuestBinaryStage,
    ) -> Result<GuestBinaryTicket, RunError> {
        use crate::workspace_session::{ActivationRefusal, WorkspaceActivationReservation};
        run.can_effect()?;
        let reader = match Store::read(
            &run.cx
                .operation()
                .observe()
                .map_err(|_| RunError::broken("stage needs observation"))?,
            run.root,
        ) {
            Observed::Verified(reader, _) => reader,
            _ => return Err(RunError::unavailable("stage journal unreadable")),
        };
        let reservation =
            match WorkspaceActivationReservation::recover(&reader, &run.owner.resource) {
                Observed::Verified(Some(reservation), _)
                    if reservation.id() == stage.reservation =>
                {
                    reservation
                }
                _ => return Err(ActivationRefusal::WrongReservation.into()),
            };
        if &run.workspace_binary_stage(&reservation)? != stage {
            return Err(RunError::refused("stage differs from frozen binding"));
        }
        crate::fault_point!(IntendBefore)?;
        let sequence = run.store.transaction(&run.cx, |txn| {
            journal::check_fence(txn.conn(), &run.owner)?;
            reservation.claimed_inputs(txn.conn(), run.id, &run.owner)?;
            if let Some(existing) = journal::effects(txn.conn(), run.id)?
                .into_iter()
                .find(|effect| effect.spec.kind == GUEST_BINARY_KIND)
            {
                if existing.spec.payload != serde_json::to_value(stage)?
                    || !matches!(
                        existing.state,
                        EffectState::Intended | EffectState::Unknown | EffectState::Applied
                    )
                {
                    return Err(RunError::refused("binary stage cannot be replaced"));
                }
                return Ok(existing.sequence);
            }
            journal::insert_effect(
                txn,
                run.id,
                &EffectSpec {
                    target: run.owner.clone(),
                    kind: GUEST_BINARY_KIND.into(),
                    payload: serde_json::to_value(stage)?,
                    settlement: Settlement::Retain,
                    required: true,
                },
            )
        })?;
        crate::fault_point!(IntendAfter)?;
        Ok(GuestBinaryTicket(EffectTicket {
            run: run.id,
            owner: run.owner.clone(),
            sequence,
        }))
    }

    /// Only verified readback can complete a stage; a partial install remains
    /// Unknown. Recovery may hash an existing exact object and submit readback
    /// again, without copying or overwriting anything.
    pub fn guest_binary_staged(
        run: &mut Run<'_>,
        ticket: &GuestBinaryTicket,
        observed: Observed<crate::workspace_session::GuestBinaryReceipt>,
    ) -> Result<(), RunError> {
        run.can_effect()?;
        if ticket.0.run != run.id || ticket.0.owner != run.owner {
            return Err(RunError::refused("foreign binary-stage ticket"));
        }
        let effect = journal::effect(run.store.conn(), run.id, ticket.0.sequence)?;
        if effect.spec.kind != GUEST_BINARY_KIND {
            return Err(RunError::refused("foreign binary-stage effect"));
        }
        let stage: crate::workspace_session::GuestBinaryStage =
            serde_json::from_value(effect.spec.payload)?;
        let receipt = match observed {
            Observed::Verified(receipt, _) => receipt,
            Observed::Unknown(reason) => {
                Self::guest_binary_unknown(run, ticket, reason)?;
                return Err(RunError::refused("binary stage readback unknown"));
            }
            Observed::Absent(_) => {
                Self::guest_binary_unknown(
                    run,
                    ticket,
                    UnknownReason::Inconsistent("binary object absent".into()),
                )?;
                return Err(RunError::refused("binary stage object absent"));
            }
        };
        if receipt.target != stage.binding.target
            || receipt.platform != stage.binding.platform
            || receipt.object != stage.binding.artifact
        {
            Self::guest_binary_unknown(
                run,
                ticket,
                UnknownReason::Inconsistent("binary stage readback mismatch".into()),
            )?;
            return Err(RunError::refused(
                "exact-target digest/platform readback differs",
            ));
        }
        if effect.state != EffectState::Applied {
            run.applied(&ticket.0, serde_json::to_value(receipt)?)?;
        }
        run.verified_workspace_binary()?;
        if !run.completed.contains_key(&ticket.0.sequence) {
            run.completed(&ticket.0, crate::outcome::RequiredObligation::Applied)?;
        }
        Ok(())
    }

    pub fn guest_binary_unknown(
        run: &mut Run<'_>,
        ticket: &GuestBinaryTicket,
        reason: UnknownReason,
    ) -> Result<(), RunError> {
        run.unknown(&ticket.0, reason)
    }
}

/// Inspect a required stage whose exact-object readback is still unresolved.
/// Retained uncertain effects remain visible after settlement; this read never
/// replays a copy or treats an interrupted owner's absence as success.
pub fn unfinished_guest_binary(
    reader: &StoreReader,
    environment: &Fence,
) -> Observed<Option<crate::workspace_session::GuestBinaryStage>> {
    let result = (|| -> Result<_, RunError> {
        let mut query = reader
            .conn()
            .prepare("SELECT run_id FROM runs WHERE owner=?1 AND generation=?2")?;
        let ids = query
            .query_map(
                params![
                    environment.resource.name().to_string(),
                    environment.generation.get() as i64
                ],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let mut stages = Vec::new();
        for id in ids {
            let id = id
                .parse()
                .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?;
            for effect in journal::effects(reader.conn(), id)? {
                if effect.spec.kind == GUEST_BINARY_KIND
                    && effect.state != EffectState::Applied
                    && effect.receipt.is_none()
                {
                    let stage: crate::workspace_session::GuestBinaryStage =
                        serde_json::from_value(effect.spec.payload)?;
                    if stage.environment != *environment || stage.run != id {
                        return Err(RunError::broken("foreign stage in environment journal"));
                    }
                    stages.push(stage);
                }
            }
        }
        if stages.len() > 1 {
            return Err(RunError::broken("ambiguous unfinished binary stage"));
        }
        Ok(stages.pop())
    })();
    match result {
        Ok(Some(stage)) => {
            Observed::Verified(Some(stage), crate::workspace_session::session_evidence())
        }
        Ok(None) => Observed::Absent(crate::observe::AbsenceProof::NotInInventory {
            inventory: crate::observe::InventoryRef {
                source: "unfinished workspace guest binary stages".into(),
                generation: environment.generation,
            },
        }),
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}
