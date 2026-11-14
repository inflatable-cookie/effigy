//! The durable activation reservation a successor generation commits through.
//!
//! A successor environment generation E must be known before the session is
//! admitted, because the canonical gate order takes `Gate(Task)` before
//! `Gate(Env)`. The reservation records E, the session is admitted against E
//! under `Gate(Task)` alone, and the activation run at E claims the reservation
//! and commits the session mapping, `active(E)` and the session's `Borrow`
//! attachment in one transaction.
use super::*;
/// The kernel table one activation reservation is stored in.
pub(crate) const RESERVATIONS: &str = "workspace_activation_reservations";

pub(crate) fn activation_evidence() -> Evidence {
    Evidence::new("kernel workspace activation reservation".into(), None)
        .expect("fixed evidence source")
}

/// The identity of one activation reservation: the first 128 bits of a digest
/// over the environment and the reserved generation. A retry derives the same
/// value, so a recovered reservation and a fresh one that names the same
/// generation are the same reservation. It names a reservation and grants
/// nothing by being known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ActivationReservationId(run::RequestId);

impl ActivationReservationId {
    fn derived(environment: &ResourceId, generation: Generation) -> Self {
        let mut material = environment.canonical();
        material.extend_from_slice(&generation.get().to_be_bytes());
        material.extend_from_slice(b"effigy.workspace-activation-reservation.v1");
        let digest = crate::id::Digest::of(&material);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.0[..16]);
        Self(run::RequestId::new(bytes))
    }
    pub fn parse(text: &str) -> Result<Self, run::InvalidHexId> {
        run::RequestId::parse(text).map(Self)
    }
    pub fn bytes(&self) -> &[u8; 16] {
        self.0.bytes()
    }
}
impl std::fmt::Display for ActivationReservationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The reservation's own lifecycle. What an activation adds to it — the
/// environment run admitted at E and the Task-owned session staged against E —
/// are separate durable facts, because the session is staged *before* the
/// environment run is admitted, and either can arrive first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationReservationState {
    /// E is the environment's next generation and no activation committed it.
    Reserved,
    /// The mapping, `active(E)` and the session's attachment committed.
    Committed,
    /// The caller abandoned it before commit. An admitted generation is never reused.
    Cancelled,
}
impl ActivationReservationState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Committed => "committed",
            Self::Cancelled => "cancelled",
        }
    }
    /// A state this kernel does not know is refused, never guessed.
    fn parse(text: &str) -> Option<Self> {
        match text {
            "reserved" => Some(Self::Reserved),
            "committed" => Some(Self::Committed),
            "cancelled" => Some(Self::Cancelled),
            _ => None,
        }
    }
}

/// The Task-owned successor session staged against a reservation: the exact
/// workspace owner that will serve the environment, the run identity that admits
/// it, and its own owner generation S — never the environment generation E.
///
/// It is frozen at staging, so the guest publication that follows can only ever
/// be validated against these three facts, and a competing Task run cannot swap
/// the session the activation will publish.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionBinding {
    pub owner: ResourceId,
    pub run: RunId,
    pub generation: Generation,
}

/// The guest publication acknowledgement for one frozen workspace activation.
/// The producer must return the session identity it actually published, not a
/// plan the host fills in after receiving a receipt for unrelated bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspacePublicationReceipt {
    pub publication: run::GuestPublicationReceipt,
    pub activation_run: RunId,
    pub session: SessionBinding,
}

/// One durable activation reservation, as a reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationReservationRecord {
    pub id: ActivationReservationId,
    pub environment: ResourceId,
    /// E: the environment generation this reservation names.
    pub generation: Generation,
    pub state: ActivationReservationState,
    /// The environment activation run admitted at E, once it is claimed.
    pub environment_run: Option<RunId>,
    /// The Task-owned session staged against E, before any commit.
    pub session: Option<SessionBinding>,
    /// The publish inputs `decide` froze when it reserved E. Absent on a
    /// reservation made without a decision (`reserve`), which is why such a
    /// generation is never `Attach`ed by a later decision.
    pub inputs: Option<PublishInputs>,
}

/// The container and VM boot one activation targets, as observed before the
/// decision and re-observed by the caller that owns the guest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerIdentity {
    pub container_id: String,
    pub vm_boot_id: String,
    pub backend: String,
}

/// One candidate from the apply inventory. Fresh targets must carry the
/// project and creating activation's identity; an unrelated container is never
/// adopted merely because it is the only one the backend currently reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TargetCandidate {
    pub identity: ContainerIdentity,
    pub project: String,
    pub created_by: Option<RunId>,
}

/// What a decision knows about the desired successor target. It is absent
/// before a fresh apply or an authorized recreation of a non-live predecessor.
/// The successor must be a new ID created by this activation, with the recorded
/// project/backend. A live protected predecessor cannot be recreated (052 §8.11;
/// Effigy Supervisor owner ruling, 2026-10-31).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TargetEvidence {
    /// Re-observed under the decision's gate.
    Observed(ContainerIdentity),
    /// The desired successor is unknown until apply, even if an authorized
    /// non-live predecessor exists. Binding requires this run's exact new ID.
    Absent { project: String, backend: String },
}
impl TargetEvidence {
    fn container(&self) -> Option<&ContainerIdentity> {
        match self {
            Self::Observed(identity) => Some(identity),
            Self::Absent { .. } => None,
        }
    }
}

/// The artifact one decision publishes. A fresh project cannot name a digest
/// before the image platform selects one, so absence is its own evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ArtifactEvidence {
    /// Reactivation: the digest this Prepare already resolved.
    Selected { digest: Digest, object: String },
    /// Fresh project: nothing is selected until the image platform is read.
    Unselected {
        image_reference: String,
        backend: String,
    },
}
impl ArtifactEvidence {
    pub fn selected(&self) -> Option<&Digest> {
        match self {
            Self::Selected { digest, .. } => Some(digest),
            Self::Unselected { .. } => None,
        }
    }
}

/// The whole published evidence one activation decision compares (052 §8.11).
/// It is frozen into the reservation before anything is applied, so a retry
/// compares the inputs the first attempt recorded and a competing caller
/// cannot attach to a session that was published for other inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishInputs {
    /// The artifact this Prepare publishes, or the honest statement that it is
    /// not selected yet.
    pub artifact: ArtifactEvidence,
    /// The resolved selection digest the generation names.
    pub selection: Digest,
    /// The frozen policy, profile and backend the render was made from.
    pub inputs_digest: Digest,
    /// What the decision knows about the target.
    pub target: TargetEvidence,
    /// Optional image facts already observed when deciding. Fresh projects bind later.
    #[serde(default)]
    pub platform: Option<ImagePlatform>,
    /// Guest-local materialisation frozen by the decision. Legacy rows remain readable but cannot publish without it.
    #[serde(default)]
    pub guest: Option<GuestContextInputs>,
}
impl PublishInputs {
    pub(crate) fn validate(&self) -> Result<(), RunError> {
        self.guest
            .as_ref()
            .ok_or_else(|| RunError::refused("workspace decisions require frozen guest inputs"))?
            .validate()?;
        if let Some(platform) = &self.platform {
            platform.validate()?;
        }
        if let ArtifactEvidence::Selected { object, .. } = &self.artifact {
            if object.trim().is_empty() {
                return Err(ActivationRefusal::ReceiptIncomplete.into());
            }
        }
        match &self.target {
            TargetEvidence::Observed(identity) => {
                if identity.container_id.trim().is_empty()
                    || identity.vm_boot_id.trim().is_empty()
                    || identity.backend.trim().is_empty()
                {
                    return Err(ActivationRefusal::ReceiptIncomplete.into());
                }
            }
            TargetEvidence::Absent { project, backend } => {
                if project.trim().is_empty() || backend.trim().is_empty() {
                    return Err(ActivationRefusal::ReceiptIncomplete.into());
                }
            }
        }
        Ok(())
    }
}

/// What one atomic activation decision chose.
#[derive(Debug, Clone)]
pub enum ActivationDecision {
    /// The current generation already serves these exact inputs: change nothing.
    Attach(SessionRef),
    /// Nothing serves these inputs: E is reserved (or recovered) for them.
    Activate(Box<WorkspaceActivationReservation>),
}

pub(crate) type ReservationRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<i64>,
    Option<String>,
);

pub(crate) fn reservation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReservationRow> {
    Ok((
        row.get::<_, i64>(0)?,
        row.get::<_, String>(1)?,
        row.get::<_, String>(2)?,
        row.get::<_, Option<String>>(3)?,
        row.get::<_, Option<String>>(4)?,
        row.get::<_, Option<String>>(5)?,
        row.get::<_, Option<i64>>(6)?,
        row.get::<_, Option<String>>(7)?,
    ))
}

pub(crate) fn decode_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    row: Option<ReservationRow>,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let Some((
        generation,
        id,
        state,
        environment_run,
        session_owner,
        session_run,
        session_generation,
        inputs,
    )) = row
    else {
        return Ok(None);
    };
    let generation = Generation::new(generation as u64);
    let Some(state) = ActivationReservationState::parse(&state) else {
        return Err(RunError::broken(format!(
            "a stored activation reservation state (`{state}`)"
        )));
    };
    let record = ActivationReservationRecord {
        id: ActivationReservationId::parse(&id).map_err(|error| {
            RunError::broken(format!("a stored activation reservation id ({error})"))
        })?,
        environment: environment.clone(),
        generation,
        state,
        environment_run: environment_run
            .map(|run| {
                run.parse()
                    .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))
            })
            .transpose()?,
        session: match (session_owner, session_run, session_generation) {
            (Some(owner), Some(run), Some(generation)) => Some(SessionBinding {
                owner: owner_identity(conn, &owner)?,
                run: run
                    .parse()
                    .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
                generation: Generation::new(generation as u64),
            }),
            (None, None, None) => None,
            // Half a binding is not a binding: a reader refuses rather than
            // inventing the owner, the run or the generation it is missing.
            _ => {
                return Err(RunError::broken(
                    "a stored activation session binding is half recorded",
                ))
            }
        },
        // A record this kernel cannot read is Broken, never a decision made on
        // guessed inputs: a later `Attach` would otherwise compare nothing.
        inputs: inputs
            .map(|json| {
                serde_json::from_str(&json).map_err(|error| {
                    RunError::broken(format!("a stored activation publish input ({error})"))
                })
            })
            .transpose()?,
    };
    if record.state == ActivationReservationState::Committed
        && (record.environment_run.is_none() || record.session.is_none())
    {
        return Err(RunError::broken(
            "a committed activation reservation names no environment run or session",
        ));
    }
    Ok(Some(record))
}

pub(crate) fn read_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    generation: Generation,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let row = conn
        .query_row(
            "SELECT environment_generation, reservation_id, state, environment_run, \
             session_owner, session_run, session_generation, inputs \
             FROM workspace_activation_reservations \
             WHERE environment=?1 AND environment_generation=?2",
            params![environment.name().to_string(), generation.get() as i64],
            reservation_row,
        )
        .optional()?;
    decode_reservation(conn, environment, row)
}

/// Re-read an exact reservation in the caller's snapshot. Missing or mismatched
/// records never grant a claim, publication or recovery authority.
pub(crate) fn require_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    generation: Generation,
    id: ActivationReservationId,
) -> Result<ActivationReservationRecord, RunError> {
    let stored = read_reservation(conn, environment, generation)?
        .ok_or_else(|| RunError::broken("the activation reservation vanished"))?;
    if stored.id != id {
        return Err(ActivationRefusal::WrongReservation.into());
    }
    Ok(stored)
}

impl ActivationReservationRecord {
    pub(crate) fn require_reserved(&self) -> Result<(), RunError> {
        match self.state {
            ActivationReservationState::Reserved => Ok(()),
            ActivationReservationState::Committed => {
                Err(ActivationRefusal::AlreadyCommitted.into())
            }
            ActivationReservationState::Cancelled => Err(ActivationRefusal::Cancelled.into()),
        }
    }
}

/// Never hide an unfinished fence behind a later empty reservation. This also
/// recovers journals written before retries preferred their pending predecessor.
pub(crate) fn recoverable_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let row = conn
        .query_row(
            "SELECT environment_generation, reservation_id, state, environment_run, \
         session_owner, session_run, session_generation, inputs \
         FROM workspace_activation_reservations WHERE environment=?1 \
         ORDER BY state IN ('committed','cancelled'), \
         CASE WHEN state NOT IN ('committed','cancelled') THEN environment_generation END ASC, \
         environment_generation DESC LIMIT 1",
            [environment.name().to_string()],
            reservation_row,
        )
        .optional()?;
    decode_reservation(conn, environment, row)
}

/// Whether the live mapping already serves exactly these inputs, and if so the
/// session a caller attaches to. Every fact is re-read here, under the same
/// `Gate(Env)` hold that would reserve a successor, so a decision cannot be
/// based on a session another activation replaced in between: the mapping, the
/// committed reservation that names it, the selection that generation
/// committed, the publication it recorded and the inputs it was decided on.
///
/// A `None` answer is `Activate`, never an attach to a near match. A target
/// that disagrees with the published container or boot is a refusal, because
/// superseding a live generation would hide a replaced container behind a new
/// one; an unfinished selection is a refusal for the same reason.
pub(crate) fn attachable(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    live: &SessionRow,
    publishing: &PublishInputs,
) -> Result<Option<SessionRef>, RunError> {
    let Some(record) = read_reservation(conn, environment, live.environment_generation)? else {
        return Ok(None);
    };
    if record.state != ActivationReservationState::Committed {
        return Ok(None);
    }
    let Some(binding) = record.session.clone() else {
        return Ok(None);
    };
    if binding.run != live.run || binding.generation != live.session_generation {
        return Ok(None);
    }
    let Some(obligation) =
        selection::read_obligation(conn, environment, live.environment_generation)?
    else {
        return Ok(None);
    };
    match obligation.state() {
        SelectionState::Pending => return Err(ActivationRefusal::SelectionPending.into()),
        SelectionState::Abandoned => return Ok(None),
        SelectionState::Switched => {}
    }
    // The environment still serves this generation: the resource row is the
    // one the commit published and nothing has moved it since.
    let active = conn
        .query_row(
            "SELECT state='active' AND last_applied=?2 FROM resources WHERE name=?1",
            params![
                environment.name().to_string(),
                live.environment_generation.get().to_string()
            ],
            |row| row.get::<_, Option<bool>>(0),
        )?
        .unwrap_or(false);
    if !active {
        return Ok(None);
    }
    // Only a decision that froze inputs can be attached to: a reservation made
    // by `reserve` recorded no evidence to compare, so its generation is
    // superseded rather than guessed at.
    let Some(frozen) = record.inputs.clone() else {
        return Ok(None);
    };
    if frozen.guest.is_none() || publishing.guest.is_none() {
        return Ok(None);
    }
    let publication = obligation.receipt().publication();
    // The target is fenced *first*, before the artifact or selection digest can
    // decide anything: a replaced container or changed boot is refused by name
    // rather than hidden behind a superseding generation.
    if let TargetEvidence::Observed(frozen_target) = &frozen.target {
        if publication.publication.container_id != frozen_target.container_id
            || publication.publication.vm_boot_id != frozen_target.vm_boot_id
        {
            return Err(ActivationRefusal::TargetEvidenceMismatch.into());
        }
    }
    if let Some(observed) = publishing.target.container() {
        if publication.publication.container_id != observed.container_id
            || publication.publication.vm_boot_id != observed.vm_boot_id
        {
            return Err(ActivationRefusal::TargetEvidenceMismatch.into());
        }
    }
    if frozen.guest != publishing.guest
        || publishing
            .platform
            .as_ref()
            .is_some_and(|platform| frozen.platform.as_ref() != Some(platform))
        || frozen.selection != publishing.selection
        || frozen.inputs_digest != publishing.inputs_digest
    {
        return Ok(None);
    }
    let Some(digest) = publishing.artifact.selected() else {
        return Ok(None);
    };
    let named = publication
        .publication
        .artifact_digest
        .strip_prefix("sha256:")
        .unwrap_or(&publication.publication.artifact_digest);
    if named != digest.hex() {
        return Ok(None);
    }
    if obligation
        .receipt()
        .selection()
        .objects
        .context_generation
        .get()
        != publication.publication.context_generation
    {
        return Ok(None);
    }
    Ok(Some(SessionRef::new(
        live.run,
        Fence {
            resource: binding.owner,
            generation: binding.generation,
        },
        environment.clone(),
        live.environment_generation,
    )))
}

pub(crate) fn activation_observe(cx: &EffectContext) -> Result<ObserveContext, RunError> {
    cx.operation().observe().map_err(|_| {
        RunError::broken("a workspace activation reservation needs an observing effect context")
    })
}

pub(crate) fn activation_reader(
    cx: &ObserveContext,
    root: &StateRoot,
) -> Result<StoreReader, RunError> {
    match Store::read(cx, root) {
        Observed::Verified(reader, _) => Ok(reader),
        Observed::Absent(_) => Err(RunError::broken("the kernel store is not readable")),
        Observed::Unknown(reason) => Err(RunError::unavailable(format!(
            "the kernel store is not readable: {reason}"
        ))),
    }
}

/// A stored owner *name* resolved to the identity it names, through the
/// resource row's canonical bytes. A name is a digest, so it is never decoded on
/// its own: a stored owner the journal does not hold is broken, not guessed.
pub(crate) fn owner_identity(
    conn: &rusqlite::Connection,
    name: &str,
) -> Result<ResourceId, RunError> {
    let canonical: Option<Vec<u8>> = conn
        .query_row(
            "SELECT canonical FROM resources WHERE name=?1",
            [name],
            |row| row.get(0),
        )
        .optional()?;
    let canonical = canonical
        .ok_or_else(|| RunError::broken("a stored session owner is not in this journal"))?;
    decode_owner(&canonical)
}

/// Why a reservation, claim, staging or commit was refused. Every answer here is
/// a rule refusing a well-formed request, so each converts to
/// [`RunError::Refused`]; a record this kernel cannot read is `Broken` instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationRefusal {
    /// The resource a caller reserved is not an environment.
    NotAnEnvironment,
    /// The claim or commit names another environment than the reservation.
    WrongEnvironment,
    /// The run was not admitted at the reserved generation E.
    WrongGeneration,
    /// The stored reservation is not the one the caller holds.
    WrongReservation,
    /// The reservation already committed its generation.
    AlreadyCommitted,
    /// The reservation was cancelled.
    Cancelled,
    /// The reservation already names another environment activation run.
    ClaimedByAnotherRun,
    /// Another live session is already staged against this generation.
    SessionAlreadyStaged,
    /// The staged session run settled, closed or moved off its generation.
    SessionGone,
    /// The staged run is not a Task-owned session ahead of its environment.
    WrongSessionOwner,
    /// The activation committed a session mapping that is not in the store.
    SessionAlreadyCommitted,
    /// Nothing is staged yet, so there is no plan to commit.
    NotStaged,
    /// The activation run recorded no applied guest publication.
    Unpublished,
    /// The guest publication receipt names another fence than the plan's E.
    ReceiptMismatch,
    /// The guest publication receipt is missing an identity it promised.
    ReceiptIncomplete,
    /// The reservation's environment run is still live, so it cannot be cancelled.
    RunStillLive,
    /// The activation claimed the reservation, so its staged session is frozen.
    Frozen,
    /// The activation run cannot act: it is settling, closed or poisoned.
    ActivationNotReady,
    /// A required effect of the activation run is not applied, so nothing may
    /// be published from it.
    ActivationIncomplete,
    /// The target the decision observed disagrees with the live publication.
    TargetEvidenceMismatch,
    /// The apply inventory did not identify exactly one target.
    TargetAmbiguous,
    /// The committed generation's selection is unfinished, so nothing may
    /// attach to it or reserve around it.
    SelectionPending,
    /// The committed generation's selection was abandoned and revoked.
    SelectionAbandoned,
    /// A selection receipt does not name the obligation's exact targets.
    SelectionMismatch,
    /// A selection is already finished and cannot be completed twice.
    SelectionSettled,
    /// A workspace activation must commit through `commit_selection`: the
    /// generic commit cannot publish a generation with no durable selection.
    SelectionRequired,
    /// The generation was reserved without a decision, so no publish inputs
    /// were frozen and nothing may be published under it.
    Unfrozen,
    /// The exact successor is still running or unsettled, so its generation is
    /// not abandoned around it.
    SuccessorRunning,
    /// Recovery has no live listener for a selection it would have to complete.
    ServingLost,
    /// A selection could still be completed, so it cannot be abandoned.
    SelectionStillCompletable,
}
impl ActivationRefusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotAnEnvironment => "not_an_environment",
            Self::WrongEnvironment => "wrong_environment",
            Self::WrongGeneration => "wrong_generation",
            Self::WrongReservation => "wrong_reservation",
            Self::AlreadyCommitted => "already_committed",
            Self::Cancelled => "cancelled",
            Self::ClaimedByAnotherRun => "claimed_by_another_run",
            Self::SessionAlreadyStaged => "session_already_staged",
            Self::SessionGone => "session_gone",
            Self::WrongSessionOwner => "wrong_session_owner",
            Self::SessionAlreadyCommitted => "session_already_committed",
            Self::NotStaged => "not_staged",
            Self::Unpublished => "unpublished",
            Self::ReceiptMismatch => "receipt_mismatch",
            Self::ReceiptIncomplete => "receipt_incomplete",
            Self::RunStillLive => "run_still_live",
            Self::Frozen => "frozen",
            Self::ActivationNotReady => "activation_not_ready",
            Self::ActivationIncomplete => "activation_incomplete",
            Self::TargetEvidenceMismatch => "target_evidence_mismatch",
            Self::TargetAmbiguous => "target_ambiguous",
            Self::SelectionPending => "selection_pending",
            Self::SelectionAbandoned => "selection_abandoned",
            Self::SelectionMismatch => "selection_mismatch",
            Self::SelectionSettled => "selection_settled",
            Self::SelectionRequired => "selection_required",
            Self::Unfrozen => "unfrozen",
            Self::SuccessorRunning => "successor_running",
            Self::ServingLost => "serving_lost",
            Self::SelectionStillCompletable => "selection_still_completable",
        }
    }
}
impl std::fmt::Display for ActivationRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Self::NotAnEnvironment => {
                "a workspace activation reserves an environment, and this resource is not one"
            }
            Self::WrongEnvironment => {
                "this activation reservation names another environment than the run it was handed"
            }
            Self::WrongGeneration => {
                "the activation run was not admitted at the generation the reservation names"
            }
            Self::WrongReservation => {
                "the stored activation reservation is not the one this caller holds"
            }
            Self::AlreadyCommitted => {
                "this environment generation already committed its workspace activation; reserve the next generation instead"
            }
            Self::Cancelled => {
                "this activation reservation was cancelled; reserve the environment again"
            }
            Self::ClaimedByAnotherRun => {
                "this activation reservation already names another environment run"
            }
            Self::SessionAlreadyStaged => {
                "this activation reservation already has a live staged workspace session"
            }
            Self::SessionGone => {
                "the staged workspace session is no longer an open run at its staged generation"
            }
            Self::WrongSessionOwner => {
                "a workspace session is a Task-owned run that comes before its environment in gate order"
            }
            Self::SessionAlreadyCommitted => {
                "this environment generation already records another workspace session"
            }
            Self::NotStaged => {
                "the activation has no staged workspace session to commit"
            }
            Self::Unpublished => {
                "the activation run has not recorded the guest publication its commit requires"
            }
            Self::ReceiptMismatch => {
                "the guest publication receipt names another environment generation than the reservation"
            }
            Self::ReceiptIncomplete => {
                "the guest publication receipt is missing an identity or a context generation"
            }
            Self::RunStillLive => {
                "the activation run at this generation is still live; settle it before cancelling its reservation"
            }
            Self::Frozen => {
                "the activation claimed this reservation, so its staged workspace session is frozen; cancel the reservation and reserve the environment again"
            }
            Self::ActivationNotReady => {
                "the environment activation run cannot act: it is settling, closed or poisoned, so it publishes nothing"
            }
            Self::ActivationIncomplete => {
                "the environment activation run still has a required effect that is not applied, so it publishes nothing"
            }
            Self::TargetAmbiguous => "the apply inventory must identify exactly one target",
            Self::TargetEvidenceMismatch => {
                "the target this activation observed is not the container and boot the live publication names; re-prepare instead of re-targeting it"
            }
            Self::SelectionPending => {
                "this generation committed a workspace selection that is still unfinished; complete or abandon it before attaching or reserving around it"
            }
            Self::SelectionAbandoned => {
                "this generation's workspace selection was abandoned and its publication revoked; reserve the next generation"
            }
            Self::SelectionMismatch => {
                "the selection receipt does not name the exact bytes, generation and listener this obligation committed"
            }
            Self::SelectionSettled => {
                "this workspace selection is already finished; a selection completes or abandons exactly once"
            }
            Self::SelectionRequired => {
                "a workspace activation commit records the guest selection it published; use commit_selection with the validated receipt"
            }
            Self::Unfrozen => {
                "this environment generation was reserved without an activation decision, so no publish inputs were frozen; decide it before publishing"
            }
            Self::SuccessorRunning => {
                "the committed successor is still running or unsettled; stop and settle it before abandoning its generation"
            }
            Self::ServingLost => {
                "the committed session has no live verified listener, so its selection cannot be completed; abandon it instead"
            }
            Self::SelectionStillCompletable => {
                "the committed session, its target and its listener are all live, so this selection can still be completed and is not abandoned"
            }
        };
        f.write_str(text)
    }
}
impl From<ActivationRefusal> for RunError {
    fn from(refusal: ActivationRefusal) -> Self {
        Self::Refused(refusal.to_string())
    }
}

/// One environment's successor activation: a durable reservation of the
/// environment generation E the next activation will reach, taken before the
/// Task-owned session is admitted so the session can be labelled with E with
/// `Gate(Env)` free.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceActivationReservation {
    pub(crate) record: ActivationReservationRecord,
}

impl WorkspaceActivationReservation {
    fn stored(&self, conn: &rusqlite::Connection) -> Result<ActivationReservationRecord, RunError> {
        require_reservation(conn, self.environment(), self.generation(), self.id())
    }

    /// Reserve `environment`'s next generation E.
    ///
    /// `Gate(Env)` is taken for this commit and relinquished before it returns,
    /// so a caller admits the Task-owned session next without holding Env. A
    /// reservation that names the environment's current next generation is
    /// recovered rather than duplicated: its id, its bound runs and its staged
    /// session all come back, so a retry continues where the first attempt
    /// stopped. A reservation whose generation the environment has since passed
    /// stays as evidence and a new one is written for the new next generation.
    pub fn reserve(
        cx: &EffectContext,
        root: &StateRoot,
        environment: &ResourceId,
        wait: Wait,
    ) -> Result<Self, RunError> {
        if environment.kind() != ResourceKind::Env {
            return Err(ActivationRefusal::NotAnEnvironment.into());
        }
        let _gate = Gate::acquire(cx, root, std::slice::from_ref(environment), wait)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let record = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            if selection::pending_obligation(txn.conn(), environment)?.is_some() {
                return Err(ActivationRefusal::SelectionPending.into());
            }
            Self::reserve_under(txn, environment, None)
        })?;
        Ok(Self { record })
    }

    /// Test the live publication and reserve its successor under one
    /// `Gate(Env)` hold (052 §8.11, post-#484 correction).
    ///
    /// `reserve` alone cannot decide this: two callers that both observed
    /// `Absent` would reserve two generations, and a caller that attached
    /// without re-reading the live session could attach to a mapping another
    /// activation had just replaced. Here the live mapping, its committed
    /// reservation, the selection it committed, the publication it recorded
    /// and the frozen inputs it was decided on are all read in the same
    /// transaction that decides, so:
    ///
    /// - identical inputs attach without advancing any generation;
    /// - different inputs, a different artifact or an abandoned selection
    ///   reserve (or recover) exactly the next generation E;
    /// - a target that disagrees with the published container or boot refuses
    ///   [`ActivationRefusal::TargetEvidenceMismatch`] instead of silently
    ///   superseding or re-targeting the live session;
    /// - an unfinished selection obligation refuses
    ///   [`ActivationRefusal::SelectionPending`]: no later attach or
    ///   reservation may bypass it.
    pub fn decide(
        cx: &EffectContext,
        root: &StateRoot,
        environment: &ResourceId,
        publishing: &PublishInputs,
        wait: Wait,
    ) -> Result<ActivationDecision, RunError> {
        if environment.kind() != ResourceKind::Env {
            return Err(ActivationRefusal::NotAnEnvironment.into());
        }
        publishing.validate()?;
        let _gate = Gate::acquire(cx, root, std::slice::from_ref(environment), wait)?;
        Self::decide_held(cx, root, environment, publishing)
    }

    /// Observe the publication inputs under the same environment gate that
    /// protects the decision. The read-only observer owns its inputs and is
    /// bounded by `deadline`; a late or failed result cannot be committed.
    pub fn decide_observing(
        cx: &EffectContext,
        root: &StateRoot,
        environment: &ResourceId,
        wait: Wait,
        deadline: std::time::Instant,
        observe: impl FnOnce(std::time::Instant) -> Result<PublishInputs, RunError> + Send + 'static,
    ) -> Result<ActivationDecision, RunError> {
        if environment.kind() != ResourceKind::Env {
            return Err(ActivationRefusal::NotAnEnvironment.into());
        }
        if std::time::Instant::now() >= deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        let bounded = cx.child_until(deadline);
        let _gate = Gate::acquire(&bounded, root, std::slice::from_ref(environment), wait)?;
        bounded
            .operation()
            .check("workspace decision observation")?;
        let effective_deadline = match bounded.operation().deadline() {
            crate::ctx::Deadline::At(until) => until.min(deadline),
            crate::ctx::Deadline::Open(_) => deadline,
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("workspace-decision-observation".into())
            .spawn(move || {
                let _ = sender.send(observe(effective_deadline));
            })
            .map_err(|error| {
                RunError::unavailable(format!(
                    "workspace decision observer could not start: {error}"
                ))
            })?;
        let publishing = loop {
            bounded
                .operation()
                .check("workspace decision observation")?;
            let remaining = effective_deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(RunError::unavailable("workspace decision deadline expired"));
            }
            match receiver.recv_timeout(remaining.min(std::time::Duration::from_millis(25))) {
                Ok(result) => break result?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RunError::unavailable(
                        "workspace decision observer exited without a result",
                    ));
                }
            }
        };
        if std::time::Instant::now() >= effective_deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        bounded
            .operation()
            .check("workspace decision observation")?;
        publishing.validate()?;
        Self::decide_held(&bounded, root, environment, &publishing)
    }

    /// Test-only candidate: neither callback is yet constrained to bounded,
    /// read-only I/O, so this does not grant production observation authority.
    #[cfg(test)]
    pub(crate) fn decide_observing_split<T: Send + 'static>(
        cx: &EffectContext,
        root: &StateRoot,
        environment: &ResourceId,
        wait: Wait,
        deadline: std::time::Instant,
        observe: impl FnOnce(std::time::Instant) -> Result<T, RunError> + Send + 'static,
        finalize: impl FnOnce(T, std::time::Instant) -> Result<PublishInputs, RunError>,
    ) -> Result<ActivationDecision, RunError> {
        if environment.kind() != ResourceKind::Env {
            return Err(ActivationRefusal::NotAnEnvironment.into());
        }
        if std::time::Instant::now() >= deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        let bounded = cx.child_until(deadline);
        let _gate = Gate::acquire(&bounded, root, std::slice::from_ref(environment), wait)?;
        bounded
            .operation()
            .check("workspace decision observation")?;
        let effective_deadline = match bounded.operation().deadline() {
            crate::ctx::Deadline::At(until) => until.min(deadline),
            crate::ctx::Deadline::Open(_) => deadline,
        };
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("workspace-decision-observation".into())
            .spawn(move || {
                let _ = sender.send(observe(effective_deadline));
            })
            .map_err(|error| {
                RunError::unavailable(format!(
                    "workspace decision observer could not start: {error}"
                ))
            })?;
        let external = loop {
            bounded
                .operation()
                .check("workspace decision observation")?;
            let remaining = effective_deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(RunError::unavailable("workspace decision deadline expired"));
            }
            match receiver.recv_timeout(remaining.min(std::time::Duration::from_millis(25))) {
                Ok(result) => break result?,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(RunError::unavailable(
                        "workspace decision observer exited without a result",
                    ));
                }
            }
        };
        if std::time::Instant::now() >= effective_deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        bounded
            .operation()
            .check("workspace decision finalization")?;
        let publishing = finalize(external, effective_deadline)?;
        if std::time::Instant::now() >= effective_deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        bounded
            .operation()
            .check("workspace decision finalization")?;
        publishing.validate()?;
        if std::time::Instant::now() >= effective_deadline {
            return Err(RunError::unavailable("workspace decision deadline expired"));
        }
        bounded.operation().check("workspace decision")?;
        Self::decide_held(&bounded, root, environment, &publishing)
    }

    fn decide_held(
        cx: &EffectContext,
        root: &StateRoot,
        environment: &ResourceId,
        publishing: &PublishInputs,
    ) -> Result<ActivationDecision, RunError> {
        crate::fault_point!(ActivationDecisionBefore)?;
        // The live session's *owner* liveness is observed under the same gate,
        // before the deciding transaction: a mapped run whose owner gate is free
        // is an interrupted session, not a live one, and Unknown liveness grants
        // nothing. The transaction re-reads the mapping and only attaches to the
        // same run this observation named.
        let owner = {
            let observe = cx.operation().observe().map_err(|_| {
                RunError::broken("an activation decision needs an observing context")
            })?;
            // A store that has never been written has no live session to
            // observe: `Absent` here is an absence of evidence, not a broken
            // journal, and the deciding transaction creates what it needs.
            let reader = match crate::store::Store::read(&observe, root) {
                Observed::Verified(reader, _) => Some(reader),
                Observed::Absent(_) => None,
                Observed::Unknown(reason) => {
                    return Err(RunError::unavailable(format!(
                        "the kernel store is not readable: {reason}"
                    )))
                }
            };
            match reader {
                Some(reader) => live_session_row(reader.conn(), environment)?.map(|live| {
                    let liveness = reader.run_owner_state(&observe, root, &live.run);
                    (live.run, liveness)
                }),
                None => None,
            }
        };
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let decision = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            txn.resource(environment, "environment")?;
            // A committed selection that is still unfinished fences every
            // decision, whatever its owner's liveness: the recovery order is
            // settle/revoke/abandon, and no attach or successor may bypass it.
            if selection::pending_obligation(txn.conn(), environment)?.is_some() {
                return Err(ActivationRefusal::SelectionPending.into());
            }
            if let Some(live) = live_session_row(txn.conn(), environment)? {
                match &owner {
                    Some((
                        run,
                        crate::observe::Observed::Verified(crate::gate::OwnerState::Held, _),
                    )) if *run == live.run => {
                        if let Some(session) =
                            attachable(txn.conn(), environment, &live, publishing)?
                        {
                            return Ok::<_, RunError>(ActivationDecision::Attach(session));
                        }
                    }
                    Some((_, crate::observe::Observed::Unknown(reason))) => {
                        return Err(RunError::unavailable(format!(
                            "the live workspace session's owner liveness is unknown: {reason}"
                        )))
                    }
                    // Interrupted, closed or absent: no live session to reuse.
                    _ => {}
                }
            }
            let record = Self::reserve_under(txn, environment, Some(publishing))?;
            Ok::<_, RunError>(ActivationDecision::Activate(Box::new(Self { record })))
        })?;
        crate::fault_point!(ActivationDecisionAfter)?;
        Ok(decision)
    }

    /// The reservation step both `reserve` and `decide` share, so the two entry
    /// points cannot disagree about which generation E is or which row recovers.
    fn reserve_under(
        txn: &mut Txn<'_>,
        environment: &ResourceId,
        inputs: Option<&PublishInputs>,
    ) -> Result<ActivationReservationRecord, RunError> {
        // The environment's row exists from its first reservation, exactly as
        // it exists from a run's admission: a first activation reserves
        // generation 1, and a reserve that is never claimed leaves only an
        // environment record at generation 0.
        txn.resource(environment, "environment")?;
        let current: Option<i64> = txn
            .conn()
            .query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [environment.name().to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let Some(current) = current else {
            return Err(RunError::broken(
                "the environment row is absent inside the transaction that wrote it",
            ));
        };
        crate::fault_point!(ReservationBefore)?;
        let generation = Generation::new(current as u64)
            .next()
            .ok_or_else(|| RunError::refused("the environment generation cannot advance"))?;
        let existing = recoverable_reservation(txn.conn(), environment)?;
        let record = match existing {
            Some(existing)
                if existing.state == ActivationReservationState::Reserved
                    && existing.generation <= generation =>
            {
                // Recovery, not a second decision: keep the inputs the first
                // attempt froze. A caller that decided on other inputs is a
                // competing activation, and it meets the reserved fence. A row
                // reserved before any decision has nothing frozen yet, so the
                // first decision to reach it freezes them here — before
                // anything is applied — and every later one must match.
                match (&existing.inputs, inputs) {
                    (Some(frozen), Some(inputs)) if frozen != inputs => {
                        // A replaced container or a changed boot is refused by
                        // name, not hidden behind a superseding generation.
                        if let (
                            TargetEvidence::Observed(frozen_target),
                            TargetEvidence::Observed(observed),
                        ) = (&frozen.target, &inputs.target)
                        {
                            if frozen_target.container_id != observed.container_id
                                || frozen_target.vm_boot_id != observed.vm_boot_id
                            {
                                return Err(ActivationRefusal::TargetEvidenceMismatch.into());
                            }
                        }
                        return Err(RunError::refused(
                            "this environment generation is reserved by a workspace activation with other publish inputs; wait for that activation or cancel its reservation",
                        ));
                    }
                    (None, Some(inputs)) => {
                        txn.conn().execute(
                            "UPDATE workspace_activation_reservations SET inputs=?3,updated_at=?4 \
                             WHERE environment=?1 AND environment_generation=?2 AND state='reserved' \
                             AND inputs IS NULL",
                            params![
                                environment.name().to_string(),
                                generation.get() as i64,
                                serde_json::to_string(inputs)?,
                                txn.now()
                            ],
                        )?;
                        return Ok(ActivationReservationRecord {
                            inputs: Some(inputs.clone()),
                            ..existing
                        });
                    }
                    _ => {}
                }
                Ok(existing)
            }
            Some(existing) if existing.generation > generation => Err(RunError::broken(
                "a stored activation reservation is ahead of the environment generation",
            )),
            Some(existing)
                if existing.generation == generation
                    && existing.state != ActivationReservationState::Cancelled =>
            {
                Ok(existing)
            }
            Some(existing) if existing.generation == generation => {
                // A cancelled reservation whose generation no run ever took is
                // revived: its id is derived from the same pair, and its inputs
                // are the ones this decision froze.
                txn.conn().execute(
                    "DELETE FROM workspace_activation_attempts WHERE reservation_id=?1",
                    [existing.id.to_string()],
                )?;
                txn.conn().execute(
                    "UPDATE workspace_activation_reservations SET state='reserved',session_owner=NULL,session_run=NULL,session_generation=NULL,inputs=?3,updated_at=?4 \
                     WHERE environment=?1 AND environment_generation=?2",
                    params![
                        environment.name().to_string(),
                        generation.get() as i64,
                        inputs.map(serde_json::to_string).transpose()?,
                        txn.now()
                    ],
                )?;
                Ok(ActivationReservationRecord {
                    state: ActivationReservationState::Reserved,
                    session: None,
                    inputs: inputs.cloned(),
                    ..existing
                })
            }
            _ => Self::insert(txn, environment, generation, inputs),
        }?;
        crate::fault_point!(ReservationAfter)?;
        Ok(record)
    }

    pub(crate) fn insert(
        txn: &mut Txn<'_>,
        environment: &ResourceId,
        generation: Generation,
        inputs: Option<&PublishInputs>,
    ) -> Result<ActivationReservationRecord, RunError> {
        let id = ActivationReservationId::derived(environment, generation);
        txn.conn().execute(
            "INSERT INTO workspace_activation_reservations \
             (environment, environment_generation, reservation_id, state, inputs, created_at, updated_at) \
             VALUES (?1,?2,?3,'reserved',?4,?5,?5)",
            params![
                environment.name().to_string(),
                generation.get() as i64,
                id.to_string(),
                inputs.map(serde_json::to_string).transpose()?,
                txn.now()
            ],
        )?;
        Ok(ActivationReservationRecord {
            id,
            environment: environment.clone(),
            generation,
            state: ActivationReservationState::Reserved,
            environment_run: None,
            session: None,
            inputs: inputs.cloned(),
        })
    }

    /// The oldest unfinished reservation, or the most recent terminal one.
    /// A later empty fence never hides the bound runs recovery must settle.
    pub fn recover(
        reader: &StoreReader,
        environment: &ResourceId,
    ) -> Observed<Option<WorkspaceActivationReservation>> {
        match recoverable_reservation(reader.conn(), environment) {
            Ok(Some(record)) => Observed::Verified(Some(Self { record }), activation_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: RESERVATIONS.into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Freeze the apply's one exact container and boot under the claimed Env
    /// owner's gate. Unknown inventory and zero or several candidates refuse.
    /// Repeating the same observation returns the original binding; no retry
    /// can replace it. The inventory must attest this run created a fresh
    /// target, or match the decision's already observed target exactly.
    pub fn bind_target(
        &self,
        authority: &mut Run<'_>,
        candidates: Observed<Vec<TargetCandidate>>,
    ) -> Result<ContainerIdentity, RunError> {
        authority.bind_workspace_target(self, candidates)
    }

    pub(crate) fn verify_claimed_run(
        &self,
        conn: &rusqlite::Connection,
        run: RunId,
        fence: &Fence,
    ) -> Result<ActivationReservationRecord, RunError> {
        if fence.resource != self.record.environment || fence.generation != self.record.generation {
            return Err(ActivationRefusal::WrongGeneration.into());
        }
        crate::run::check_workspace_fence(conn, fence)?;
        let stored = self.stored(conn)?;
        if stored.state != ActivationReservationState::Reserved {
            return Err(ActivationRefusal::Frozen.into());
        }
        if stored.environment_run != Some(run) {
            return Err(ActivationRefusal::ClaimedByAnotherRun.into());
        }
        Ok(stored)
    }

    pub(crate) fn claimed_inputs(
        &self,
        conn: &rusqlite::Connection,
        run: RunId,
        fence: &Fence,
    ) -> Result<PublishInputs, RunError> {
        self.verify_claimed_run(conn, run, fence)?
            .inputs
            .ok_or_else(|| ActivationRefusal::Unfrozen.into())
    }

    pub fn id(&self) -> ActivationReservationId {
        self.record.id
    }
    /// E: the reserved successor environment generation.
    pub fn generation(&self) -> Generation {
        self.record.generation
    }
    pub fn environment(&self) -> &ResourceId {
        &self.record.environment
    }
    pub fn state(&self) -> ActivationReservationState {
        self.record.state
    }
    pub fn record(&self) -> &ActivationReservationRecord {
        &self.record
    }
    /// The publish inputs `decide` froze when it reserved E, if it decided.
    pub fn inputs(&self) -> Option<&PublishInputs> {
        self.record.inputs.as_ref()
    }

    /// `claim(reservation_id, EnvRun)`: the claim a caller hands to
    /// [`Run::begin_claimed`], which binds this reservation to the environment
    /// activation run in that run's own admission transaction. The claim names
    /// no run yet, and admission refuses a reservation that already names
    /// another one, so a claim can bind exactly one activation run.
    pub fn claim(&self) -> Result<ActivationClaim, RunError> {
        match self.record.state {
            ActivationReservationState::Reserved => {
                Ok(ActivationClaim::Environment(EnvironmentClaim {
                    reservation: self.record.id,
                    environment: self.record.environment.clone(),
                    generation: self.record.generation,
                }))
            }
            ActivationReservationState::Committed => {
                Err(ActivationRefusal::AlreadyCommitted.into())
            }
            ActivationReservationState::Cancelled => Err(ActivationRefusal::Cancelled.into()),
        }
    }

    /// `claim_session(reservation)`: the right to admit the *frozen* successor
    /// session this reservation staged, at its own generation S.
    ///
    /// The identity it names was frozen by [`Self::stage_session_binding`], so
    /// only that run can be admitted and only it can be published. Nothing is
    /// attached and no mapping is written; the successor's admission takes its
    /// private liveness slot, so a predecessor may remain live and authoritative.
    pub fn claim_session(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
    ) -> Result<SessionClaim, RunError> {
        self.claim_session_with_attempt(cx, root, None)
    }

    pub(crate) fn claim_session_with_attempt(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        attempt: Option<run::RequestId>,
    ) -> Result<SessionClaim, RunError> {
        let observe = activation_observe(cx)?;
        let reader = activation_reader(&observe, root)?;
        let stored = self.stored(reader.conn())?;
        check_staging_attempt(reader.conn(), stored.id, attempt)?;
        stored.require_reserved()?;
        let Some(binding) = stored.session else {
            return Err(RunError::from(ActivationRefusal::NotStaged));
        };
        Ok(SessionClaim {
            reservation: stored.id,
            environment: stored.environment,
            generation: stored.generation,
            binding,
            attempt,
        })
    }

    /// `stage_session_binding(reservation, run, S)`: freeze the Task-owned
    /// successor session this activation will publish — the exact workspace
    /// owner, the run identity that will admit it, and its own owner generation
    /// S (never the environment generation E).
    ///
    /// It takes no gate on the workspace owner and admits no run, so it never
    /// touches the live predecessor session: the predecessor keeps its run, its
    /// attachment, its selected bytes and its listener while the successor is
    /// staged. S is reserved above the owner's current and prior reserved
    /// session generations. The Task counter advances only with the activation
    /// commit; admission authenticates the exact staged run. The staging is exclusive: a
    /// competing Task run cannot replace a session that is staged or admitted,
    /// and once the activation claims the reservation the frozen identity can
    /// never change again.
    pub fn stage_session_binding(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        owner: &ResourceId,
        run: RunId,
    ) -> Result<SessionBinding, RunError> {
        self.stage_with_attempt(cx, root, owner, run, None)
    }

    /// Stage the exact session and its creating attempt in one transaction.
    pub fn stage_for_attempt(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        owner: &ResourceId,
        run: RunId,
        attempt: run::RequestId,
    ) -> Result<SessionBinding, RunError> {
        self.stage_with_attempt(cx, root, owner, run, Some(attempt))
    }

    fn stage_with_attempt(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        owner: &ResourceId,
        run: RunId,
        attempt: Option<run::RequestId>,
    ) -> Result<SessionBinding, RunError> {
        check_session_owner(&self.record, owner)?;
        crate::fault_point!(SessionStageBefore)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let frozen = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            // The owner's row exists from its first staging, exactly as the
            // environment's exists from its first reservation, so the frozen
            // identity can always be read back to the identity it names.
            txn.resource(owner, "workspace session")?;
            let stored = self.stored(txn.conn())?;
            let prior_attempt: Option<String> = txn.conn().query_row(
                "SELECT attempt_id FROM workspace_activation_attempts WHERE reservation_id=?1",
                [stored.id.to_string()], |row| row.get(0),
            ).optional()?;
            match (stored.session.is_some(), prior_attempt.as_deref(), attempt) {
                (true, Some(saved), Some(offered)) if saved == offered.to_string() => {},
                (true, None, None) => {},
                (false, None, _) => {},
                _ => return Err(RunError::refused("competing attempt or untagged reservation; reservation remains recoverable")),
            }
            if stored.state == ActivationReservationState::Reserved && stored.environment_run.is_none() {
                if let Some(existing) = &stored.session {
                    if existing.owner == *owner && existing.run == run {
                        return Ok(existing.clone());
                    }
                }
            }
            let run_exists: bool = txn.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id=?1)",
                [run.to_string()], |row| row.get(0),
            )?;
            if run_exists {
                return Err(RunError::refused("session run identity already exists; reservation remains recoverable"));
            }
            let base = next_generation(txn.conn(), owner)?.get() - 1;
            let highest: i64 = txn.conn().query_row(
                "SELECT coalesce(max(generation),0) FROM workspace_session_runs WHERE owner=?1",
                [owner.name().to_string()], |row| row.get(0),
            )?;
            let generation = Generation::new(base.max(highest as u64)).next()
                .ok_or_else(|| RunError::refused("the workspace session generation cannot advance"))?;
            let binding = SessionBinding { owner: owner.clone(), run, generation };
            let borrowed: bool = txn.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM attachments WHERE run_id=?1 AND state<>'released' AND environment<>?2)",
                params![run.to_string(), self.record.environment.name().to_string()],
                |row| row.get(0),
            )?;
            if borrowed {
                return Err(RunError::refused(
                    "the staged workspace session already borrows another environment",
                ));
            }
            stored.require_reserved()?;
            // The activation has admitted its environment run: its publication
            // may already have happened, so the session it will publish is
            // frozen. This is the replacement-after-publication refusal.
            if stored.environment_run.is_some() {
                return Err(RunError::from(ActivationRefusal::Frozen));
            }
            let changed = match stored.session {
                Some(existing) if existing == binding => 1,
                Some(existing) if existing.owner != binding.owner => {
                    return Err(RunError::from(ActivationRefusal::WrongSessionOwner))
                }
                Some(_) => {
                    return Err(RunError::from(ActivationRefusal::SessionAlreadyStaged));
                }
                None => txn.conn().execute(
                    "UPDATE workspace_activation_reservations \
                     SET session_owner=?3,session_run=?4,session_generation=?5,updated_at=?6 \
                     WHERE environment=?1 AND environment_generation=?2 AND state='reserved' \
                     AND session_run IS NULL",
                    params![
                        self.record.environment.name().to_string(),
                        self.record.generation.get() as i64,
                        binding.owner.name().to_string(),
                        binding.run.to_string(),
                        binding.generation.get() as i64,
                        txn.now(),
                    ],
                )?,
            };
            if let Some(attempt) = attempt {
                txn.conn().execute(
                    "INSERT INTO workspace_activation_attempts(reservation_id,attempt_id) VALUES (?1,?2)",
                    params![stored.id.to_string(), attempt.to_string()],
                )?;
            }
            // Exactly one writer stages this session: a competing Task run's
            // staging loses here rather than replacing the winner.
            if changed != 1 {
                return Err(RunError::from(ActivationRefusal::SessionAlreadyStaged));
            }
            let mut aliases = txn.conn().prepare(
                "SELECT DISTINCT res.canonical FROM runs r JOIN resources res ON res.name=r.owner \
                 WHERE r.owner<>?1 AND (r.phase<>'closed' OR coalesce(r.cleanup_outcome,'incomplete')<>'complete') \
                 UNION SELECT res.canonical FROM workspace_session_runs s JOIN resources res ON res.name=s.owner \
                 JOIN workspace_activation_reservations a ON a.reservation_id=s.reservation_id \
                 WHERE s.owner<>?1 AND a.state='reserved'",
            )?;
            for canonical in aliases.query_map([owner.name().to_string()], |row| row.get::<_, Vec<u8>>(0))? {
                if decode_owner(&canonical?)?.checkout_alias_of(owner) {
                    return Err(ActivationRefusal::WrongSessionOwner.into());
                }
            }
            drop(aliases);
            // One staging per exact workspace owner, even across environments.
            let competing: bool = txn.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM workspace_session_runs s \
                 JOIN workspace_activation_reservations a ON a.reservation_id=s.reservation_id \
                 WHERE s.owner=?1 AND a.state='reserved' AND a.reservation_id<>?2)",
                params![owner.name().to_string(), stored.id.to_string()], |row| row.get(0),
            )?;
            if competing { return Err(ActivationRefusal::SessionAlreadyStaged.into()); }
            // Authenticate the existing mapping before preserving its run authority.
            // No run, holder, gate or effect is stopped or retargeted here.
            if let Some(previous) = live_session_row(txn.conn(), &self.record.environment)? {
                let previous_owner: String = txn.conn().query_row(
                    "SELECT owner FROM runs WHERE run_id=?1", [previous.run.to_string()], |row| row.get(0),
                )?;
                if previous_owner != owner.name().to_string() {
                    return Err(ActivationRefusal::WrongSessionOwner.into());
                }
                txn.conn().execute(
                    "INSERT OR IGNORE INTO workspace_session_runs \
                     (run_id,owner,generation,reservation_id,base_generation,gate_key) VALUES (?1,?2,?3,NULL,?4,NULL)",
                    params![previous.run.to_string(), previous_owner, previous.session_generation.get() as i64, base as i64],
                )?;
            }
            // A generic unresolved Task is never admitted around. Only sessions
            // whose identity was already authenticated by this seam may overlap.
            let generic: bool = txn.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM runs r LEFT JOIN workspace_session_runs s ON s.run_id=r.run_id \
                 WHERE r.owner=?1 AND (r.phase<>'closed' OR coalesce(r.cleanup_outcome,'incomplete')<>'complete') \
                 AND s.run_id IS NULL)", [owner.name().to_string()], |row| row.get(0),
            )?;
            if generic { return Err(ActivationRefusal::SessionAlreadyStaged.into()); }
            let slot = session_slot(owner, run)?;
            txn.conn().execute(
                "INSERT INTO workspace_session_runs \
                 (run_id,owner,generation,reservation_id,base_generation,gate_key) VALUES (?1,?2,?3,?4,?5,?6)",
                params![run.to_string(), owner.name().to_string(), generation.get() as i64,
                    stored.id.to_string(), base as i64, slot.name().to_string()],
            )?;
            Ok::<_, RunError>(binding)
        })?;
        crate::fault_point!(SessionStageAfter)?;
        Ok(frozen)
    }

    /// The staged plan, read and validated without writing anything: the
    /// environment run admitted at E, the session staged at S, and the fence a
    /// guest publication receipt must name. `Absent` means nothing is staged
    /// yet or the staged run is gone; `Unknown` means the record is unreadable.
    pub fn plan(&self, reader: &StoreReader) -> Observed<Option<ActivationPlan>> {
        let stored = match read_reservation(
            reader.conn(),
            &self.record.environment,
            self.record.generation,
        ) {
            Ok(stored) => stored,
            Err(error) => {
                return Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string()))
            }
        };
        let Some(stored) = stored else {
            return Observed::Absent(AbsenceProof::NoRow {
                table: RESERVATIONS.into(),
            });
        };
        if stored.id != self.record.id {
            return Observed::Unknown(UnknownReason::Inconsistent(
                "workspace activation reservation identity changed".into(),
            ));
        }
        match activation_plan(reader.conn(), &stored) {
            Ok(Some(plan)) => Observed::Verified(Some(plan), activation_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: RESERVATIONS.into(),
            }),
            Err(RunError::Refused(_)) => Observed::Absent(AbsenceProof::NoRow {
                table: "runs".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Commit the staged activation under the environment run's held
    /// `Gate(Env)`: validate that the plan names exactly this admitted run and
    /// its staged session, validate the guest publication receipt the run
    /// recorded against that plan, and then write `active(E)`, the
    /// environment's session mapping and the session's `Borrow` attachment in
    /// one transaction.
    ///
    /// A commit that has already happened for this reservation answers with the
    /// same session, so a retry after a lost answer is not a second commit. A
    /// borrowed successor Run must stay in this caller through the commit,
    /// retaining its Task slot from admission before Env was acquired. A
    /// crash before commit leaves the predecessor authoritative and the
    /// reservation recoverable; the successor is never advertised halfway.
    pub fn commit(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        environment_run: &Run<'_>,
        session_run: &Run<'_>,
    ) -> Result<CommittedActivation, RunError> {
        let _ = (cx, root, environment_run, session_run);
        // The generic commit published `active(E)`, the mapping and the session's
        // `Borrow` with no record of the guest bytes that generation selects,
        // which is exactly the window §8.11 closes. There is one commit path:
        // `commit_selection`.
        Err(ActivationRefusal::SelectionRequired.into())
    }

    /// The one commit: re-validate the stored plan, the held runs and the
    /// validated selection receipt, then write `active(E)`, the session
    /// mapping, the session's `Borrow` attachment and a `Pending` selection
    /// obligation in one transaction, or none of them.
    pub(super) fn commit_with(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        environment_run: &Run<'_>,
        session_run: &Run<'_>,
        selection: &SelectionReceipt,
        launch: &SessionLaunch,
    ) -> Result<CommittedActivation, RunError> {
        let fence = environment_run.fence().clone();
        if fence.resource != self.record.environment {
            return Err(RunError::from(ActivationRefusal::WrongEnvironment));
        }
        if fence.generation != self.record.generation {
            return Err(RunError::from(ActivationRefusal::WrongGeneration));
        }
        // The plan is the frozen staging plus the environment run that claims
        // it, validated by reading everything the commit will use: the frozen
        // successor identity and its owning Run. The borrowed Run retains its
        // private Task slot across this entire commit; death of its process
        // necessarily aborts this transaction too. No Task gate is probed or
        // acquired while the environment run holds Env.
        let (plan, frozen) = {
            let observe = activation_observe(cx)?;
            let reader = activation_reader(&observe, root)?;
            let stored = self.stored(reader.conn())?;
            let plan = activation_plan(reader.conn(), &stored)?
                .ok_or_else(|| RunError::from(ActivationRefusal::NotStaged))?;
            if !session_run.owns_workspace_session(root, &plan.session)? {
                return Err(ActivationRefusal::SessionGone.into());
            }
            (plan, stored.inputs)
        };
        let receipt = plan.validate_publication(environment_run)?;
        let staged_binary = environment_run.verified_workspace_binary()?;
        if staged_binary.reservation != self.id() {
            return Err(ActivationRefusal::WrongReservation.into());
        }
        if receipt.publication.container_id != staged_binary.binding.target.container_id
            || receipt.publication.vm_boot_id != staged_binary.binding.target.vm_boot_id
            || selection.selection().backend != staged_binary.binding.target.backend
            || selection.selection().objects.binary != staged_binary.binding.artifact
        {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        selection.require_validated_document()?;
        selection.validate(&plan)?;
        // The commit holds the session's live custody: the launch that published
        // this lease, for this session, with the key and listener still in hand.
        // A receipt alone cannot publish a session nothing is serving.
        if launch.binding() != *plan.binding() {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        let custody = launch.hold_lease(selection.serving())?;
        // The decision froze its inputs before anything was applied. Publishing
        // is the moment they are due: the artifact and target this run
        // acknowledged must be exactly the ones the decision recorded, or the
        // generation would serve bytes nobody decided on.
        let Some(frozen) = frozen else {
            return Err(ActivationRefusal::Unfrozen.into());
        };
        let selected = selection.selection().objects.binary.digest.clone();
        // A fresh project cannot name an artifact before its image platform is
        // read, so its decision records `Unselected`: this commit freezes the
        // artifact the run actually selected, once, in its own transaction. Any
        // later activation compares against that frozen digest.
        if let ArtifactEvidence::Selected { digest, .. } = &frozen.artifact {
            if digest != &selected {
                return Err(ActivationRefusal::SelectionMismatch.into());
            }
        }
        // The resolved selection the decision froze is the one the published
        // context document names. A context prepared for another selection is
        // never published under this decision.
        if selection.selection_digest() != &frozen.selection {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        if let TargetEvidence::Observed(identity) = &frozen.target {
            if receipt.publication.container_id != identity.container_id
                || receipt.publication.vm_boot_id != identity.vm_boot_id
            {
                return Err(ActivationRefusal::TargetEvidenceMismatch.into());
            }
        }
        // The receipt the commit publishes is the one the run recorded: a
        // selection built against another publication names other bytes.
        if selection.publication() != &receipt {
            return Err(ActivationRefusal::SelectionMismatch.into());
        }
        let plan_before = plan.clone();
        crate::fault_point!(ActivationCommitBefore)?;
        crate::fault_point!(SelectionCommitBefore)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let committed = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            run::check_activation_fence(txn.conn(), &fence)?;
            let stored = self.stored(txn.conn())?;
            // The frozen staged identity is what the receipt was authenticated
            // against: the exact owner, run and S the staging froze, never a
            // session that appeared or changed afterwards.
            let plan = activation_plan(txn.conn(), &stored)?
                .ok_or_else(|| RunError::from(ActivationRefusal::NotStaged))?;
            if plan != plan_before {
                return Err(RunError::from(ActivationRefusal::Frozen));
            }
            if plan.environment_run != Some(environment_run.id()) {
                return Err(RunError::from(ActivationRefusal::ClaimedByAnotherRun));
            }
            plan.validate_receipt(&receipt)?;
            // Publish all bound facts once, atomically with the selection.
            if stored.state == ActivationReservationState::Reserved {
                let mut bound_inputs = stored.inputs.clone().ok_or(ActivationRefusal::Unfrozen)?;
                bound_inputs.target = TargetEvidence::Observed(staged_binary.binding.target.clone());
                bound_inputs.platform = Some(staged_binary.binding.platform.clone());
                bound_inputs.artifact = ArtifactEvidence::Selected { digest: staged_binary.binding.artifact.digest.clone(), object: staged_binary.binding.artifact.path.clone() };
                txn.conn().execute("UPDATE workspace_activation_reservations SET inputs=?3,updated_at=?4 WHERE environment=?1 AND environment_generation=?2 AND state='reserved'", params![self.environment().name().to_string(), self.generation().get() as i64, serde_json::to_string(&bound_inputs)?, txn.now()])?;
            }
            if stored.state == ActivationReservationState::Committed {
                // A retry answers with the obligation the first commit wrote. A
                // retry that names different bytes is refused: the committed
                // generation's selection is frozen.
                let obligation =
                    selection::read_obligation(txn.conn(), &self.record.environment, self.record.generation)?;
                if let Some(stored) = obligation.as_ref() {
                    if selection.selection() != stored.receipt().selection()
                        || selection.selection_digest() != stored.receipt().selection_digest()
                    {
                        return Err(ActivationRefusal::SelectionMismatch.into());
                    }
                }
                // A committed reservation without its obligation is a record
                // this kernel cannot explain; it never answers as success.
                if obligation.is_none() {
                    return Err(RunError::broken(
                        "a committed workspace activation records no selection obligation",
                    ));
                }
                return Ok(CommittedActivation { plan, obligation });
            }
            let existing: Option<(String, i64)> = txn
                .conn()
                .query_row(
                    "SELECT run_id, environment_generation FROM workspace_sessions WHERE environment=?1",
                    [self.record.environment.name().to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?;
            if let Some((_run, environment_generation)) = existing {
                // A successor replaces the environment's mapping. The
                // predecessor session keeps running and keeps its attachment:
                // a new generation revokes its record, never its holder.
                if environment_generation as u64 >= self.record.generation.get() {
                    return Err(RunError::from(ActivationRefusal::SessionAlreadyCommitted));
                }
            }
            let advanced = txn.conn().execute(
                "UPDATE resources SET generation=?2,updated_at=?3 WHERE name=?1 AND generation= \
                 (SELECT base_generation FROM workspace_session_runs WHERE run_id=?4 AND owner=?1)",
                params![plan.session.owner.name().to_string(), plan.session.generation.get() as i64,
                    txn.now(), plan.session.run.to_string()],
            )?;
            if advanced != 1 { return Err(ActivationRefusal::WrongGeneration.into()); }
            run::publish_active_state(txn, &fence)?;
            crate::fault_point!(ActivationMappingBefore)?;
            txn.conn().execute(
                "INSERT INTO workspace_sessions \
                 (environment, run_id, session_generation, environment_generation, created_at, updated_at) \
                 VALUES (?1,?2,?3,?4,?5,?5) ON CONFLICT(environment) DO UPDATE SET \
                 run_id=excluded.run_id, session_generation=excluded.session_generation, \
                 environment_generation=excluded.environment_generation, updated_at=excluded.updated_at",
                params![
                    self.record.environment.name().to_string(),
                    plan.session.run.to_string(),
                    plan.session.generation.get() as i64,
                    self.record.generation.get() as i64,
                    txn.now()
                ],
            )?;
            crate::fault_point!(ActivationAttachmentBefore)?;
            run::holder_attachment(txn, &fence, plan.session.run, AttachMode::Borrow)?;
            // Exactly one commit publishes this generation: a second writer
            // (another activation that somehow reached the transaction) loses
            // here instead of writing a second mapping.
            let changed = txn.conn().execute(
                "UPDATE workspace_activation_reservations SET state='committed',updated_at=?3 \
                 WHERE environment=?1 AND environment_generation=?2 AND state='reserved' \
                 AND session_run=?4 AND session_generation=?5",
                params![
                    self.record.environment.name().to_string(),
                    self.record.generation.get() as i64,
                    txn.now(),
                    plan.session.run.to_string(),
                    plan.session.generation.get() as i64
                ],
            )?;
            if changed != 1 {
                return Err(RunError::from(ActivationRefusal::SessionAlreadyCommitted));
            }
            // The selection obligation is written by this commit's own
            // transaction: there is no window in which the generation is
            // committed and no durable record of the pointers it needs.
            custody.revalidate(selection.serving())?;
            let obligation = Some(selection::record_obligation(
                txn,
                &plan,
                environment_run.id(),
                selection,
            )?);
            Ok(CommittedActivation { plan, obligation })
        })?;
        crate::fault_point!(ActivationCommitAfter)?;
        crate::fault_point!(SelectionCommitAfter)?;
        Ok(committed)
    }

    /// Abandon this reservation before its commit. A live activation run at E is
    /// refused: its admission is durable and only its own settle ends it. A
    /// generation already admitted is never reused. An unclaimed generation
    /// may be reserved again, with its prior staging explicitly cleared.
    pub fn cancel(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        wait: Wait,
    ) -> Result<Self, RunError> {
        self.cancel_with_attempt(cx, root, wait, None)
    }

    /// Only the attempt that staged this session may cancel its reservation.
    pub fn cancel_for_attempt(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        wait: Wait,
        attempt: run::RequestId,
    ) -> Result<Self, RunError> {
        self.cancel_with_attempt(cx, root, wait, Some(attempt))
    }

    fn cancel_with_attempt(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        wait: Wait,
        attempt: Option<run::RequestId>,
    ) -> Result<Self, RunError> {
        let _gate = Gate::acquire(
            cx,
            root,
            std::slice::from_ref(&self.record.environment),
            wait,
        )?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let record = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let stored = self.stored(txn.conn())?;
            let saved: Option<String> = txn
                .conn()
                .query_row(
                    "SELECT attempt_id FROM workspace_activation_attempts WHERE reservation_id=?1",
                    [stored.id.to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            match (saved.as_deref(), attempt) {
                (Some(saved), Some(offered)) if saved == offered.to_string() => {}
                (None, None) => {}
                _ => return Err(RunError::refused(
                    "competing attempt or untagged reservation; reservation remains recoverable",
                )),
            }
            match stored.state {
                ActivationReservationState::Committed => {
                    return Err(RunError::from(ActivationRefusal::AlreadyCommitted))
                }
                ActivationReservationState::Cancelled => return Ok(stored),
                ActivationReservationState::Reserved => {}
            }
            if let Some(session) = stored.session.as_ref() {
                if run::workspace_integrations_unsettled(txn.conn(), session, stored.id)? {
                    return Err(RunError::refused(
                        "integration effects unsettled; re-run up",
                    ));
                }
            }
            if let Some(run) = stored.environment_run {
                if run_needs_settlement(txn.conn(), run, true)? {
                    return Err(RunError::from(ActivationRefusal::RunStillLive));
                }
            }
            if let Some(session) = &stored.session {
                if run_needs_settlement(txn.conn(), session.run, false)? {
                    return Err(RunError::from(ActivationRefusal::RunStillLive));
                }
            }
            txn.conn().execute(
                "UPDATE workspace_activation_reservations SET state='cancelled',updated_at=?3 \
                 WHERE environment=?1 AND environment_generation=?2",
                params![
                    self.record.environment.name().to_string(),
                    self.record.generation.get() as i64,
                    txn.now()
                ],
            )?;
            Ok(ActivationReservationRecord {
                state: ActivationReservationState::Cancelled,
                ..stored
            })
        })?;
        Ok(Self { record })
    }
}
