//! Who may claim a reserved generation, and the plan one claim commits.
//!
//! A claim is one right to admit a run at a reserved generation, in one
//! admission. It is not a permission that can be replayed, shared or widened:
//! the environment claim, the session claim and the plan it produces are read
//! under the same gate and the same reservation row.
use super::*;
/// One right to admit a run at a reserved generation, in one admission
/// transaction. Only [`WorkspaceActivationReservation::claim`] and
/// [`WorkspaceActivationReservation::claim_session`] construct one, and only
/// [`Run::begin_claimed`] consumes it. A claim is not authority: admission
/// re-checks the durable reservation in its own transaction, so a claim for a
/// generation, a run or a session the journal no longer offers binds nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationClaim {
    /// The environment activation run at the reserved generation E.
    Environment(EnvironmentClaim),
    /// The frozen successor workspace session, at its own generation S.
    Session(SessionClaim),
}

impl ActivationClaim {
    pub fn reservation_id(&self) -> ActivationReservationId {
        match self {
            Self::Environment(claim) => claim.reservation,
            Self::Session(claim) => claim.reservation,
        }
    }
    /// The environment this claim belongs to.
    pub fn environment(&self) -> &ResourceId {
        match self {
            Self::Environment(claim) => &claim.environment,
            Self::Session(claim) => &claim.environment,
        }
    }
    /// E: the reserved environment generation.
    pub fn generation(&self) -> Generation {
        match self {
            Self::Environment(claim) => claim.generation,
            Self::Session(claim) => claim.generation,
        }
    }
    /// For a session claim: the frozen successor identity it admits.
    pub fn session_binding(&self) -> Option<&SessionBinding> {
        match self {
            Self::Environment(_) => None,
            Self::Session(claim) => Some(&claim.binding),
        }
    }

    /// Bind the claim to the run being admitted, inside the admission
    /// transaction. The generation is the one the owner counter advanced to, so
    /// a reservation or a staging another admission consumed refuses here.
    pub(crate) fn bind(
        &self,
        txn: &mut Txn<'_>,
        owner: &ResourceId,
        generation: Generation,
        run: RunId,
    ) -> Result<(), RunError> {
        match self {
            Self::Environment(claim) => claim.bind(txn, owner, generation, run),
            Self::Session(claim) => claim.adopt(txn, owner, generation, run),
        }
    }
}

/// The right to admit the environment activation run at the reserved generation
/// E.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentClaim {
    pub(crate) reservation: ActivationReservationId,
    pub(crate) environment: ResourceId,
    pub(crate) generation: Generation,
}

impl EnvironmentClaim {
    pub fn reservation_id(&self) -> ActivationReservationId {
        self.reservation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    /// E: the generation this claim admits at.
    pub fn generation(&self) -> Generation {
        self.generation
    }

    fn bind(
        &self,
        txn: &mut Txn<'_>,
        owner: &ResourceId,
        generation: Generation,
        run: RunId,
    ) -> Result<(), RunError> {
        if owner.kind() != ResourceKind::Env || owner != &self.environment {
            return Err(RunError::from(ActivationRefusal::WrongEnvironment));
        }
        if generation != self.generation {
            return Err(RunError::from(ActivationRefusal::WrongGeneration));
        }
        let stored = require_reservation(txn.conn(), owner, generation, self.reservation)?;
        stored.require_reserved()?;
        if let Some(claimed) = stored.environment_run {
            if claimed != run {
                return Err(RunError::from(ActivationRefusal::ClaimedByAnotherRun));
            }
            return Ok(());
        }
        txn.conn().execute(
            "UPDATE workspace_activation_reservations SET environment_run=?3,updated_at=?4 \
             WHERE environment=?1 AND environment_generation=?2 AND environment_run IS NULL",
            params![
                owner.name().to_string(),
                generation.get() as i64,
                run.to_string(),
                txn.now()
            ],
        )?;
        Ok(())
    }
}

/// The right to admit the frozen successor workspace session at its own
/// generation S. It is the *same* owner the predecessor session serves, and it
/// admits one run identity only: the one the staging froze.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionClaim {
    pub(crate) reservation: ActivationReservationId,
    pub(crate) environment: ResourceId,
    pub(crate) generation: Generation,
    pub(crate) binding: SessionBinding,
    pub(crate) attempt: Option<run::RequestId>,
}

pub(crate) fn check_staging_attempt(
    conn: &rusqlite::Connection,
    reservation: ActivationReservationId,
    attempt: Option<run::RequestId>,
) -> Result<(), RunError> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT attempt_id FROM workspace_activation_attempts WHERE reservation_id=?1",
            params![reservation.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    if stored != attempt.map(|id| id.to_string()) {
        return Err(RunError::refused(
            "competing attempt or untagged reservation; reservation remains recoverable",
        ));
    }
    Ok(())
}

impl SessionClaim {
    pub fn reservation_id(&self) -> ActivationReservationId {
        self.reservation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    /// E: the reserved environment generation.
    pub fn generation(&self) -> Generation {
        self.generation
    }
    /// The frozen successor identity: owner, run and S.
    pub fn binding(&self) -> &SessionBinding {
        &self.binding
    }

    /// Adopt the frozen identity in the successor's admission transaction. The
    /// staged owner, run and generation are compared with what admission is
    /// about to write, so a stale staging (the owner's counter moved) or a
    /// different run refuses before any run row exists.
    fn adopt(
        &self,
        txn: &mut Txn<'_>,
        owner: &ResourceId,
        generation: Generation,
        run: RunId,
    ) -> Result<(), RunError> {
        if owner.kind() != ResourceKind::Task || owner != &self.binding.owner {
            return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
        }
        if run != self.binding.run || generation != self.binding.generation {
            return Err(RunError::from(ActivationRefusal::WrongGeneration));
        }
        let stored = require_reservation(
            txn.conn(),
            &self.environment,
            self.generation,
            self.reservation,
        )?;
        check_staging_attempt(txn.conn(), self.reservation, self.attempt)?;
        stored.require_reserved()?;
        // The staging is frozen: the identity this claim names is still the one
        // the reservation holds, or the claim admits nothing.
        if stored.session.as_ref() != Some(&self.binding) {
            return Err(RunError::from(ActivationRefusal::Frozen));
        }
        let current: bool = txn.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM workspace_session_runs s JOIN resources r ON r.name=s.owner \
             WHERE s.run_id=?1 AND s.owner=?2 AND s.generation=?3 AND s.reservation_id=?4 \
             AND s.base_generation=r.generation)",
            params![run.to_string(), owner.name().to_string(), generation.get() as i64, self.reservation.to_string()],
            |row| row.get(0),
        )?;
        if !current {
            return Err(ActivationRefusal::WrongGeneration.into());
        }
        Ok(())
    }
}

/// Refuse an environment admission that would take a generation a workspace
/// activation reserved but has not claimed. This is the fence a competing
/// activation meets, and it is why a reservation made before the session is not
/// a second authority: only admission through the reservation may use E.
pub(crate) fn refuse_reserved_generation(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
    generation: Generation,
) -> Result<(), RunError> {
    if owner.kind() != ResourceKind::Env {
        return Ok(());
    }
    let reserved = read_reservation(conn, owner, generation)?
        .is_some_and(|record| record.state == ActivationReservationState::Reserved);
    if reserved {
        return Err(RunError::refused(
            "this environment generation is reserved by a workspace activation; admit the activation through its reservation, or cancel the reservation first",
        ));
    }
    Ok(())
}

/// A bound reservation may still have its owning activation alive while its
/// Env gate is temporarily released. Refuse before predecessor recovery, since
/// recovering that run would otherwise settle another owner's activation.
pub(crate) fn refuse_bound_reservation_recovery(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
) -> Result<(), RunError> {
    if owner.kind() != ResourceKind::Env {
        return Ok(());
    }
    if let Some(record) = recoverable_reservation(conn, owner)?.filter(|record| {
        record.state == ActivationReservationState::Reserved && record.environment_run.is_some()
    }) {
        if let Some(session) = record.session.as_ref() {
            if run::workspace_integrations_unsettled(conn, session, record.id)? {
                return Err(RunError::refused(
                    "integration effects unsettled; re-run up",
                ));
            }
        }
        return Err(RunError::refused(
            "this environment is reserved by a workspace activation; reservation remains recoverable",
        ));
    }
    Ok(())
}

/// Explicit settlement of a bound activation is allowed only with positive
/// evidence that the exact process which admitted it is gone. A free Env gate
/// alone is not liveness evidence during the integration interval.
pub(crate) fn verify_reserved_owner_dead(
    conn: &rusqlite::Connection,
    owner: &ResourceId,
    run: RunId,
) -> Result<(), RunError> {
    if owner.kind() != ResourceKind::Env {
        return Ok(());
    }
    let Some(reservation) = recoverable_reservation(conn, owner)? else {
        return Ok(());
    };
    if reservation.state != ActivationReservationState::Reserved
        || reservation.environment_run != Some(run)
    {
        return Ok(());
    }
    let identity: Option<String> = conn.query_row(
        "SELECT owner_identity FROM runs WHERE run_id=?1",
        [run.to_string()],
        |row| row.get(0),
    )?;
    let identity: crate::ident::ProcessIdentity = serde_json::from_str(
        identity.as_deref().ok_or_else(|| {
            RunError::unavailable("reserved activation owner identity is unavailable; reservation remains recoverable")
        })?,
    ).map_err(|_| RunError::unavailable("reserved activation owner identity is unreadable; reservation remains recoverable"))?;
    match crate::ident::process_identity(identity.pid) {
        Observed::Absent(_) => Ok(()),
        Observed::Verified(current, _) if current != identity => Ok(()),
        Observed::Verified(_, _) => Err(RunError::refused(
            "reserved workspace activation owner is still alive; reservation remains recoverable",
        )),
        Observed::Unknown(_) => Err(RunError::unavailable(
            "reserved workspace activation owner liveness is unknown; reservation remains recoverable",
        )),
    }
}

/// The guest publication an activation run recorded must be applied and carry
/// a receipt: `active` may only follow a completed required obligation (§8.11).
pub(crate) fn published_receipt(environment_run: &Run<'_>) -> Result<WorkspacePublicationReceipt, RunError> {
    let effects = environment_run.effects()?;
    let Some(effect) = effects
        .iter()
        .find(|effect| effect.spec.kind == run::GUEST_PUBLICATION_KIND)
    else {
        return Err(RunError::from(ActivationRefusal::Unpublished));
    };
    if !effect.spec.required || effect.state != run::EffectState::Applied {
        return Err(RunError::from(ActivationRefusal::Unpublished));
    }
    let receipt = effect
        .receipt
        .clone()
        .ok_or_else(|| RunError::broken("an applied guest publication recorded no receipt"))?;
    serde_json::from_value(receipt)
        .map_err(|error| RunError::broken(format!("a stored guest publication receipt ({error})")))
}

/// A session run is Task-owned and comes before its environment in canonical
/// gate order, so its attachment can never reverse the order the kernel holds.
pub(crate) fn check_session_owner(
    record: &ActivationReservationRecord,
    owner: &ResourceId,
) -> Result<(), RunError> {
    if owner.kind() != ResourceKind::Task || owner.scope() != record.environment.scope() {
        return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
    }
    if owner.canonical() >= record.environment.canonical() {
        return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
    }
    Ok(())
}

/// The plan an activation validates and writes: E, the Task-owned session
/// staged at S with the owner that comes before E, and — once the activation run
/// is admitted at E — that exact run. A plan exists as soon as a session is
/// staged, so a guest publication receipt can be validated against it before the
/// run admits and before anything is committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationPlan {
    pub(crate) reservation: ActivationReservationId,
    pub(crate) environment: ResourceId,
    pub(crate) environment_generation: Generation,
    pub(crate) environment_run: Option<RunId>,
    pub(crate) session: SessionBinding,
    pub(crate) guest: Option<GuestContextInputs>,
    pub(crate) image_platform: Option<ImagePlatform>,
}

impl ActivationPlan {
    pub fn image_platform(&self) -> Result<&ImagePlatform, RunError> {
        self.image_platform.as_ref().ok_or_else(|| {
            RunError::refused("workspace context needs the verified staged image platform")
        })
    }
    pub fn guest_context(&self) -> Result<&GuestContextInputs, RunError> {
        self.guest
            .as_ref()
            .ok_or_else(|| ActivationRefusal::Unfrozen.into())
    }
    pub fn reservation_id(&self) -> ActivationReservationId {
        self.reservation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    /// E, the environment generation this activation publishes.
    pub fn environment_generation(&self) -> Generation {
        self.environment_generation
    }
    /// The environment activation run admitted at E, once the reservation is
    /// claimed. `None` while only the session is staged.
    pub fn environment_run(&self) -> Option<RunId> {
        self.environment_run
    }
    /// The Task-owned session run staged against E.
    pub fn session_run(&self) -> RunId {
        self.session.run
    }
    /// The exact workspace owner that serves the successor session.
    pub fn session_owner(&self) -> &ResourceId {
        &self.session.owner
    }
    /// The frozen successor identity this plan publishes.
    pub fn binding(&self) -> &SessionBinding {
        &self.session
    }
    /// S, the session run's own owner generation. Never E.
    pub fn session_generation(&self) -> Generation {
        self.session.generation
    }
    /// The fence a guest publication receipt must name: the environment at E,
    /// not the session's generation.
    pub fn fence(&self) -> Fence {
        Fence {
            resource: self.environment.clone(),
            generation: self.environment_generation,
        }
    }
    /// The session identity this commit records, with E as its environment
    /// label and S as its own fence.
    pub fn session(&self) -> SessionRef {
        SessionRef::new(
            self.session.run,
            Fence {
                resource: self.session.owner.clone(),
                generation: self.session.generation,
            },
            self.environment.clone(),
            self.environment_generation,
        )
    }

    /// Authenticate the guest publication the activation run recorded against
    /// this frozen plan, before the commit publishes anything.
    ///
    /// Three fences are checked together: the receipt must name E (the
    /// environment at the reserved generation), the run that recorded it must be
    /// the plan's own environment activation run, and the session this commit
    /// will publish is the frozen `(owner, run, S)` the staging names — never a
    /// receipt from another run, generation or session. The run must also be
    /// able to publish at all, and every required effect it recorded must be
    /// applied.
    ///
    /// This is deliberately a *staging* check, not a committed-mapping check: a
    /// receipt is validated before anything is published, so a crash after the
    /// immutable guest pointer but before the commit leaves the workspace
    /// unavailable rather than advertised.
    pub fn validate_publication(
        &self,
        environment_run: &Run<'_>,
    ) -> Result<WorkspacePublicationReceipt, RunError> {
        if self.environment_run != Some(environment_run.id()) {
            return Err(ActivationRefusal::ClaimedByAnotherRun.into());
        }
        if environment_run.fence() != &self.fence() {
            return Err(ActivationRefusal::WrongGeneration.into());
        }
        // A commit publishes the environment's state, so it is an effect of this
        // run: a run that cannot act (settling, closed or poisoned) and a run
        // with any required effect still unresolved publishes nothing.
        environment_run
            .can_effect()
            .map_err(|_| RunError::from(ActivationRefusal::ActivationNotReady))?;
        if environment_run
            .effects()?
            .iter()
            .any(|effect| effect.spec.required && effect.state != run::EffectState::Applied)
        {
            return Err(RunError::from(ActivationRefusal::ActivationIncomplete));
        }
        let receipt = published_receipt(environment_run)?;
        self.validate_receipt(&receipt)?;
        Ok(receipt)
    }

    /// Validate only the receipt's own fields against this plan's fence. The
    /// full authentication is [`Self::validate_publication`].
    pub fn validate_receipt(&self, receipt: &WorkspacePublicationReceipt) -> Result<(), RunError> {
        if receipt.publication.environment != self.fence()
            || Some(receipt.activation_run) != self.environment_run
            || receipt.session != self.session
        {
            return Err(ActivationRefusal::ReceiptMismatch.into());
        }
        if receipt.publication.container_id.trim().is_empty()
            || receipt.publication.vm_boot_id.trim().is_empty()
            || receipt.publication.artifact_digest.trim().is_empty()
            || receipt.publication.context_generation == 0
        {
            return Err(ActivationRefusal::ReceiptIncomplete.into());
        }
        Ok(())
    }
}

/// A committed activation: the plan it validated, the session identity every
/// front end now resolves, and — on the workspace path — the selection
/// obligation its own transaction wrote.
#[derive(Debug, Clone)]
pub struct CommittedActivation {
    pub(crate) plan: ActivationPlan,
    pub(crate) obligation: Option<SelectionObligation>,
}

impl CommittedActivation {
    pub fn plan(&self) -> &ActivationPlan {
        &self.plan
    }
    pub fn session(&self) -> SessionRef {
        self.plan.session()
    }
}

/// Read and validate the plan a reservation's two bound runs describe.
pub(crate) fn activation_plan(
    conn: &rusqlite::Connection,
    record: &ActivationReservationRecord,
) -> Result<Option<ActivationPlan>, RunError> {
    let Some(session) = record.session.clone() else {
        return Ok(None);
    };
    let live: Option<(i64, String, Vec<u8>)> = conn
        .query_row(
            "SELECT r.generation, r.phase, res.canonical FROM runs r \
             JOIN resources res ON res.name=r.owner WHERE r.run_id=?1",
            [session.run.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((generation, phase, canonical)) = live else {
        return Err(RunError::from(ActivationRefusal::SessionGone));
    };
    if generation as u64 != session.generation.get()
        || matches!(phase.as_str(), "stopping" | "closed")
    {
        return Err(RunError::from(ActivationRefusal::SessionGone));
    }
    let session_owner = decode_owner(&canonical)?;
    // The admitted run is the frozen staging's run, under the frozen owner: a
    // session that appeared some other way is never published.
    if session_owner.canonical() != session.owner.canonical() {
        return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
    }
    check_session_owner(record, &session.owner)?;
    let borrowed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM attachments WHERE run_id=?1 AND state<>'released' AND environment<>?2)",
        params![
            session.run.to_string(),
            record.environment.name().to_string()
        ],
        |row| row.get(0),
    )?;
    if borrowed {
        return Err(RunError::from(ActivationRefusal::WrongSessionOwner));
    }
    // Read only the existing required stage's frozen binding. Fresh Up never
    // guesses image facts in its initial guest inputs before apply.
    let mut image_platform = None;
    if let Some(run) = record.environment_run {
        let mut query = conn.prepare(
            "SELECT target FROM events WHERE run_id=?1 AND state='applied' ORDER BY seq",
        )?;
        for encoded in query.query_map([run.to_string()], |row| row.get::<_, String>(0))? {
            let spec: crate::run::EffectSpec = serde_json::from_str(&encoded?)?;
            if spec.kind != crate::run::GUEST_BINARY_KIND {
                continue;
            }
            let stage: GuestBinaryStage = serde_json::from_value(spec.payload)?;
            if stage.run != run
                || stage.reservation != record.id
                || stage.environment.resource != record.environment
                || stage.environment.generation != record.generation
                || image_platform.is_some()
            {
                return Err(RunError::broken(
                    "workspace context has conflicting staged image facts",
                ));
            }
            image_platform = Some(stage.binding.platform);
        }
    }
    Ok(Some(ActivationPlan {
        image_platform,
        reservation: record.id,
        environment: record.environment.clone(),
        environment_generation: record.generation,
        environment_run: record.environment_run,
        guest: record
            .inputs
            .as_ref()
            .and_then(|inputs| inputs.guest.clone()),
        session,
    }))
}
