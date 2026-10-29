//! The environment-owned workspace session (052 §8.7) on top of the kernel
//! workspace seam (§8.11).
//!
//! An environment that declares an operator workspace has exactly one
//! `Detached` workspace session run. This module owns that session's identity:
//! which run is the environment's one session, what a front end prints for it,
//! and the down a session requested from inside. The replay-safe admission
//! record, the durable client attachments, the bounded stream cursor and the
//! bounded control path all belong to the kernel workspace seam in
//! `crate::run::workspace` (lane A's channel builds on the same records), so a
//! session consumes them instead of keeping a second copy. `crate::remote::{
//! session, stream}` remains remote *workload* custody and is never host
//! session authority.
//!
//! Nothing here spawns a process, binds a socket, signals a group or settles
//! another run: the session run is admitted through `Run::begin`, a child
//! through `Run::begin_child`, control travels as a journalled `run::request`,
//! and a down takes its permit through `run::Attachments`.
//!
//! [`WorkspaceActivationReservation`] is the one exception to "a session is
//! recorded when it is admitted": a successor environment generation E must be
//! known before the session is admitted, because the canonical gate order takes
//! `Gate(Task)` before `Gate(Env)`. It reserves E durably, the session is
//! admitted against E under `Gate(Task)` alone, and the activation run at E
//! claims the reservation and later commits the environment's session mapping,
//! `active(E)` and the session's `Borrow` attachment in one transaction. E and
//! the session's own owner generation S are always separate fences.
//!
//! One kernel rule is not negotiable here and shapes the recovery path: a
//! workspace session is a Task-owned run, and K5 admits one unresolved run per
//! owner. While the predecessor session is live it holds `Gate(Task)`, so a
//! successor session *refuses* rather than being admitted — and it is never
//! admitted by stopping the predecessor. That predecessor's holders,
//! attachments, selected bytes and listener are left exactly as they are; a
//! successor activation preserves them and only its own generation's session is
//! new. A re-Prepare whose session is still live therefore leaves the
//! environment's session record revoked by the new generation until the
//! predecessor ends.

mod down;

#[cfg(test)]
mod tests;

pub use down::{DownChildren, DownCoordination, DownHolder, DownStart};

use crate::ctx::{CleanupContext, EffectContext, ObserveContext, StopCause};
use crate::gate::{Gate, OwnerState, Wait};
use crate::id::{Generation, ResolvedInvocation, ResourceId, ResourceKind, RunId};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use crate::run::{
    self, AttachMode, Attachments, BeginOpts, Fence, Phase, Release, Run, RunError,
    SettlementAdapter,
};
use crate::store::{Store, StoreReader, Txn};
use crate::supervise::Placement;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

/// The stored attachment mode of the one run that borrows the environment.
/// `AttachMode::Borrow` is its JSON encoding, so a session is the run that
/// borrows rather than a child run that holds the environment exclusively.
const BORROW_MODE: &str = "\"Borrow\"";

/// One environment's recorded session, before it is validated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionRow {
    run: RunId,
    session_generation: Generation,
    environment_generation: Generation,
}

/// The session `environment` recorded, while that record still describes it.
/// Every field is checked against the journal: a record is evidence to
/// validate, never authority.
fn live_session_row(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    current_environment_generation: Generation,
) -> Result<Option<SessionRow>, RunError> {
    let row = conn
        .query_row(
            "SELECT run_id, session_generation, environment_generation FROM workspace_sessions \
             WHERE environment=?1",
            [environment.name().to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )
        .optional()?;
    let Some((run, session_generation, environment_generation)) = row else {
        return Ok(None);
    };
    let row = SessionRow {
        run: run
            .parse()
            .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
        session_generation: Generation::new(session_generation as u64),
        environment_generation: Generation::new(environment_generation as u64),
    };
    // A new environment generation revokes the session (§8.7): the record is
    // stale even if its run has not settled yet.
    if row.environment_generation != current_environment_generation {
        return Ok(None);
    }
    let live = conn
        .query_row(
            "SELECT generation, phase FROM runs WHERE run_id=?1",
            [row.run.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    match live {
        Some((generation, phase))
            if generation as u64 == row.session_generation.get() && phase != "closed" => {}
        _ => return Ok(None),
    }
    let borrowing = conn
        .query_row(
            "SELECT env_generation FROM attachments WHERE environment=?1 AND run_id=?2 \
             AND mode=?3 AND state<>'released'",
            params![
                environment.name().to_string(),
                row.run.to_string(),
                BORROW_MODE
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    match borrowing {
        Some(generation) if generation as u64 == row.environment_generation.get() => {}
        _ => return Ok(None),
    }
    Ok(Some(row))
}

/// Records `session` as `environment`'s workspace session. One environment has
/// one live session: an existing record that still validates refuses the write,
/// and a stale one is replaced.
fn record_session(
    cx: &EffectContext,
    root: &StateRoot,
    session: &SessionRef,
) -> Result<(), RunError> {
    let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
    store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
        let current: Option<i64> = txn
            .conn()
            .query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [session.environment().name().to_string()],
                |row| row.get(0),
            )
            .optional()?;
        let current = current.map(|value| Generation::new(value as u64));
        if current != Some(session.environment_generation()) {
            return Err(RunError::refused(
                "the environment generation moved before the session was recorded",
            ));
        }
        if let Some(existing) = live_session_row(
            txn.conn(),
            session.environment(),
            session.environment_generation(),
        )? {
            if existing.run != session.run() {
                return Err(RunError::refused(
                    "this environment already has a live workspace session",
                ));
            }
            return Ok(());
        }
        txn.conn().execute(
            "INSERT INTO workspace_sessions \
             (environment, run_id, session_generation, environment_generation, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?5) \
             ON CONFLICT(environment) DO UPDATE SET run_id=excluded.run_id, \
              session_generation=excluded.session_generation, \
              environment_generation=excluded.environment_generation, updated_at=excluded.updated_at",
            params![
                session.environment().name().to_string(),
                session.run().to_string(),
                session.generation().get() as i64,
                session.environment_generation().get() as i64,
                txn.now(),
            ],
        )?;
        Ok::<_, RunError>(())
    })
}

pub(crate) fn session_evidence() -> Evidence {
    Evidence::new("workspace session journal".into(), None).expect("fixed evidence source")
}

fn decode_owner(bytes: &[u8]) -> Result<ResourceId, RunError> {
    let id = ResourceId::decode_canonical(bytes)
        .map_err(|error| RunError::broken(format!("a stored session owner ({error})")))?;
    if id.canonical() != bytes {
        return Err(RunError::broken("a stored session owner is not canonical"));
    }
    Ok(id)
}

/// Which front end opened a terminal (§8.7). All three report one managed
/// session identity; they differ only in how they render it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalKind {
    Tui,
    Text,
    Headless,
}
impl TerminalKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tui => "tui",
            Self::Text => "text",
            Self::Headless => "headless",
        }
    }
}

/// How one client's ownership of its work ended (§8.11): EOF without an
/// acknowledged detach, the client going away, or a signal the terminal
/// received. Each is the same kernel release, and the cause the supervisor
/// stops the owned work with differs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerLoss {
    /// End of input on the terminal, with no detach acknowledged.
    EndOfInput,
    /// The terminal or its client disappeared without closing cleanly.
    Disconnected,
    /// The terminal received this signal: TERM, HUP, or an operator's own.
    Signal(i32),
}
impl OwnerLoss {
    /// The release this loss records on the owner's attachment. An EOF, a
    /// disconnect and a signal are all the client going without a detach; an
    /// acknowledged detach is not a loss and uses `Release::Detach`.
    pub fn release(self) -> Release {
        Release::Eof
    }
    /// The cause the owned work is stopped with. A signal names itself; an EOF
    /// or a disconnect is owner loss.
    pub fn cause(self) -> StopCause {
        match self {
            Self::EndOfInput | Self::Disconnected => StopCause::OwnerLost,
            Self::Signal(signal) => StopCause::Signal(signal),
        }
    }
}

/// The durable identity of one environment's workspace session: the `Detached`
/// run, the owner generation that fences every request, and the environment
/// generation the session borrows.
///
/// Every front end — a `Tui`, a plain text client and a headless one — resolves
/// the same value, so all three report one managed session identity rather than
/// one per terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRef {
    run: RunId,
    fence: Fence,
    environment: ResourceId,
    environment_generation: Generation,
}

impl SessionRef {
    /// The supervisor's own identity for the session run it just admitted. A
    /// caller that has not admitted the run resolves it with
    /// [`SessionRef::resolve`] or [`SessionRef::of_run`] instead.
    pub fn new(
        run: RunId,
        fence: Fence,
        environment: ResourceId,
        environment_generation: Generation,
    ) -> Self {
        Self {
            run,
            fence,
            environment,
            environment_generation,
        }
    }
    pub fn run(&self) -> RunId {
        self.run
    }
    pub fn fence(&self) -> &Fence {
        &self.fence
    }
    pub fn environment(&self) -> &ResourceId {
        &self.environment
    }
    pub fn environment_generation(&self) -> Generation {
        self.environment_generation
    }
    /// The session generation that keys every request id (§8.11): the session
    /// run's own owner generation, so it advances only when a new session is
    /// admitted, and a recovered request is never read against a successor.
    pub fn generation(&self) -> Generation {
        self.fence.generation
    }
    /// The kernel workspace seam's session identity, for reservation, attach
    /// and control.
    pub fn session(&self) -> run::Session {
        run::Session {
            run: self.run,
            generation: self.generation(),
        }
    }
    /// What every front end prints for this session. One session, one label.
    pub fn label(&self) -> String {
        format!(
            "workspace session {} of {} (generation {})",
            self.run,
            self.environment.label(),
            self.generation().get()
        )
    }

    /// The session of `environment`: the run its journal recorded as the
    /// workspace session, while that record still holds.
    ///
    /// A `Borrow` attachment alone does not name the session — a child run or a
    /// host `dev` hold can borrow the same environment — so the mapping is its
    /// own record, and this read validates it before use: the run is at the
    /// recorded generation and not closed, it still borrows the environment at
    /// the recorded generation, and the environment has not advanced. A record
    /// that fails any check resolves as absent, so a stale session is never
    /// mistaken for a live one and a successor can be admitted.
    pub fn resolve(reader: &StoreReader, environment: &ResourceId) -> Observed<Option<SessionRef>> {
        let result = (|| -> Result<Option<SessionRef>, RunError> {
            let generation: Option<i64> = reader
                .conn()
                .query_row(
                    "SELECT generation FROM resources WHERE name=?1",
                    [environment.name().to_string()],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(generation) = generation else {
                return Ok(None);
            };
            let current = Generation::new(generation as u64);
            let Some(row) = live_session_row(reader.conn(), environment, current)? else {
                return Ok(None);
            };
            match Self::of_run(reader, row.run) {
                Observed::Verified(session, _) => Ok(session.filter(|session| {
                    session.generation() == row.session_generation
                        && session.environment_generation() == row.environment_generation
                })),
                Observed::Absent(_) => Ok(None),
                Observed::Unknown(reason) => Err(RunError::unavailable(reason.to_string())),
            }
        })();
        match result {
            Ok(Some(session)) => Observed::Verified(Some(session), session_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "workspace_sessions".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// The session identity recorded for `run`, whatever attached it. A retry
    /// that recovered an admission resolves its run this way, so the recovered
    /// answer names the same session the first attempt admitted.
    pub fn of_run(reader: &StoreReader, run: RunId) -> Observed<Option<SessionRef>> {
        let result = (|| -> Result<Option<SessionRef>, RunError> {
            let owner = reader
                .conn()
                .query_row(
                    "SELECT r.generation, res.canonical FROM runs r \
                     JOIN resources res ON res.name=r.owner WHERE r.run_id=?1",
                    [run.to_string()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()?;
            let Some((generation, canonical)) = owner else {
                return Ok(None);
            };
            let owner = decode_owner(&canonical)?;
            let environment = reader
                .conn()
                .query_row(
                    "SELECT a.environment, a.env_generation FROM attachments a \
                     WHERE a.run_id=?1 AND a.state<>'released' \
                     ORDER BY a.environment LIMIT 1",
                    [run.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .optional()?;
            let Some((environment, environment_generation)) = environment else {
                return Ok(None);
            };
            let environment = reader
                .conn()
                .query_row(
                    "SELECT canonical FROM resources WHERE name=?1",
                    [&environment],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            let Some(environment) = environment else {
                return Ok(None);
            };
            Ok(Some(SessionRef::new(
                run,
                Fence {
                    resource: owner,
                    generation: Generation::new(generation as u64),
                },
                decode_owner(&environment)?,
                Generation::new(environment_generation as u64),
            )))
        })();
        match result {
            Ok(Some(session)) => Observed::Verified(Some(session), session_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "attachments".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Admits the environment's one workspace session run.
    ///
    /// The session is a `Detached` run, so a terminal can leave without ending
    /// it; a caller that plans any other placement is refused before anything
    /// is written. The run borrows the environment for its whole life, which is
    /// what a second entry finds and attaches to instead of duplicating, and the
    /// entry itself is guarded by the workspace entry's owner gate: request ids
    /// only exist inside a session and are reserved through the kernel workspace
    /// seam (`run::reserve`).
    #[allow(clippy::too_many_arguments)]
    pub fn admit<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        environment: &Fence,
        placement: Placement,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        session_placement(&opts, placement)?;
        let mut run = Run::begin(cx, root, invocation, opts, cleanup, adapter)?;
        Attachments::attach(&mut run, environment, AttachMode::Borrow, Wait::FailFast)?;
        let session = SessionRef::new(
            run.id(),
            run.fence().clone(),
            environment.resource.clone(),
            environment.generation,
        );
        if let Err(error) = record_session(cx, root, &session) {
            // A session the journal cannot name is not a session: settle it so
            // nothing live is left without its environment record.
            run.settle(cleanup, adapter)?;
            return Err(error);
        }
        Ok((run, session))
    }

    /// Admits the environment's one workspace session against a reserved
    /// successor generation E, before the environment activation run exists and
    /// therefore before `Gate(Env)` is held: the canonical Task-before-Env gate
    /// order survives, and nothing ever waits on a Task owner gate with Env
    /// held.
    ///
    /// Nothing is attached and no mapping is recorded here. The environment is
    /// not active at E yet, so the activation commit writes the mapping, the
    /// `Borrow` attachment and `active(E)` together from the staged binding.
    /// `E` is the reserved environment generation; the session's own owner
    /// generation S is `session.generation()`.
    #[allow(clippy::too_many_arguments)]
    pub fn admit_reserved<'a>(
        cx: &EffectContext,
        root: &'a StateRoot,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        reservation: &WorkspaceActivationReservation,
        placement: Placement,
    ) -> Result<(Run<'a>, SessionRef), RunError> {
        session_placement(&opts, placement)?;
        reservation.admits_another_session(cx, root)?;
        let run = Run::begin(cx, root, invocation, opts, cleanup, adapter)?;
        let session = SessionRef::new(
            run.id(),
            run.fence().clone(),
            reservation.environment().clone(),
            reservation.generation(),
        );
        if let Err(error) = reservation.stage_session_binding(cx, root, &run, session.generation())
        {
            // A session the activation cannot name is not a session: settle it
            // so nothing live is left holding a Task owner gate.
            run.settle(cleanup, adapter)?;
            return Err(error);
        }
        Ok((run, session))
    }
}

/// The placement a workspace session may be admitted with. A `Detached` session
/// is its own supervisor, so it plans that lifecycle rather than promising one
/// and running another.
fn session_placement(opts: &BeginOpts, placement: Placement) -> Result<(), RunError> {
    if placement != Placement::Detached {
        return Err(RunError::refused(
            "a workspace session is a Detached run, not an owning terminal",
        ));
    }
    if let Some(plan) = &opts.custody {
        if plan.placement != placement {
            return Err(RunError::broken(
                "a session custody plan and its placement disagree",
            ));
        }
    }
    Ok(())
}

/// A handle on one resolved session: durable identity, the kernel seam's
/// session value, and the down coordination a session requested from inside.
/// It carries no owning pipe, so a front end can hold it without owning the
/// session's lifetime.
#[derive(Debug, Clone)]
pub struct SessionHandle {
    session: SessionRef,
}

impl SessionHandle {
    pub fn new(session: SessionRef) -> Self {
        Self { session }
    }
    pub fn identity(&self) -> &SessionRef {
        &self.session
    }
    /// The kernel workspace seam's identity for reservation, attach and
    /// control (`run::reserve`, `run::attach_secondary`, `Attachment::control`).
    pub fn session(&self) -> run::Session {
        self.session.session()
    }

    /// The down coordination for this session (§8.7): a foreign live or
    /// `Unknown` holder refuses before anything stops.
    pub fn down(
        &self,
        cx: &ObserveContext,
        root: &StateRoot,
        reader: &StoreReader,
    ) -> Observed<DownStart> {
        DownCoordination::begin(cx, root, reader, &self.session)
    }
}

/// Sends one journalled stop request to `target` and rings its doorbell. Shared
/// by a down's child stops and the session's own settle request; the request is
/// the kernel's own control path, not a second one.
pub(crate) fn request_stop(
    cx: &EffectContext,
    root: &StateRoot,
    reader: &StoreReader,
    request: &run::RequestId,
    target: RunId,
) -> Result<RunId, RunError> {
    let fence = target_fence(reader, target)?;
    let stop = request_run_id(request)?;
    // A committed request is the durable fact: the same id naming the same stop
    // is that request, not a duplicate one. The journal refuses a second row, so
    // a retried stop answers instead of erroring.
    match recorded_request(reader, stop)? {
        Some(recorded) if recorded.run == target && recorded.expected == fence => {
            if !matches!(recorded.action, run::RequestAction::Stop) {
                return Err(RunError::refused(
                    "this request id already names another action",
                ));
            }
        }
        Some(_) => {
            return Err(RunError::refused(
                "this request id already names another run",
            ))
        }
        None => run::request(
            cx,
            root,
            &run::Request {
                id: stop,
                run: target,
                expected: fence.clone(),
                action: run::RequestAction::Stop,
            },
        )?,
    }
    if let Ok(observe) = cx
        .operation()
        .child(std::time::Duration::from_secs(1))
        .observe()
    {
        let _ = run::connect_control_socket(&observe, root, &fence.resource, target);
    }
    Ok(target)
}

/// A kernel request identity as the `RunId` a journalled control request names.
/// The bits are the request id's own; nothing about them grants authority.
fn request_run_id(request: &run::RequestId) -> Result<RunId, RunError> {
    let bytes = request.bytes();
    let mut millis = [0u8; 8];
    millis[2..].copy_from_slice(&bytes[..6]);
    let mut random = [0u8; 10];
    random.copy_from_slice(&bytes[6..]);
    RunId::new(u64::from_be_bytes(millis), random)
        .map_err(|error| RunError::broken(format!("a control request id ({error})")))
}

/// The fence a run row was admitted under, read without acquiring its gate.
fn target_fence(reader: &StoreReader, target: RunId) -> Result<Fence, RunError> {
    let (generation, canonical) = reader
        .conn()
        .query_row(
            "SELECT r.generation, res.canonical FROM runs r \
             JOIN resources res ON res.name=r.owner WHERE r.run_id=?1",
            [target.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()?
        .ok_or_else(|| RunError::refused("the requested run is not in this journal"))?;
    Ok(Fence {
        resource: decode_owner(&canonical)?,
        generation: Generation::new(generation as u64),
    })
}

/// A journalled control request by id, whatever its state. The row is the
/// durable record `run::request` wrote, so a retry can recognise its own.
fn recorded_request(reader: &StoreReader, id: RunId) -> Result<Option<run::Request>, RunError> {
    let action = reader
        .conn()
        .query_row(
            "SELECT action FROM requests WHERE request_id=?1",
            [id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    action
        .map(|action| serde_json::from_str(&action).map_err(RunError::from))
        .transpose()
}

/// The phase a run is in, for a caller that names a session in a diagnostic.
pub fn phase(reader: &StoreReader, run: RunId) -> Observed<Phase> {
    match run::summary(reader, run) {
        Observed::Verified(summary, evidence) => Observed::Verified(summary.phase, evidence),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

// ---------------------------------------------------------------------------
// Workspace activation reservation (§8.7, §8.11)
// ---------------------------------------------------------------------------
//
// The one ordering rule this section exists for: the workspace session is a
// Task-owned run, and `Gate(Task)` must be taken before `Gate(Env)`. The
// session's environment label is the *successor* generation E, which does not
// exist until the environment activation run is admitted — and admitting it
// means holding `Gate(Env)`. Reserving E durably breaks that circle: the
// reservation is taken and relinquished under `Gate(Env)`, the session is
// admitted and staged while Env is free, and the activation run claims E and
// later commits the mapping, `active(E)` and the session's `Borrow` attachment
// atomically.

/// The kernel table one activation reservation is stored in.
const RESERVATIONS: &str = "workspace_activation_reservations";

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
    /// The caller abandoned it before commit. Its generation is never reused.
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

/// The Task-owned session run staged against a reservation: its identity and
/// its own owner generation S, which is never the environment generation E.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionBinding {
    pub run: RunId,
    pub generation: Generation,
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
}

type ReservationRow = (
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
);

fn decode_reservation(
    environment: &ResourceId,
    row: Option<ReservationRow>,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let Some((generation, id, state, environment_run, session_run, session_generation)) = row
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
        session: match (session_run, session_generation) {
            (Some(run), Some(generation)) => Some(SessionBinding {
                run: run
                    .parse()
                    .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
                generation: Generation::new(generation as u64),
            }),
            (None, None) => None,
            // Half a binding is not a binding: a reader refuses rather than
            // inventing the run or the generation it is missing.
            _ => {
                return Err(RunError::broken(
                    "a stored activation session binding is half recorded",
                ))
            }
        },
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

fn read_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
    generation: Generation,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let row = conn
        .query_row(
            "SELECT environment_generation, reservation_id, state, environment_run, \
             session_run, session_generation FROM workspace_activation_reservations \
             WHERE environment=?1 AND environment_generation=?2",
            params![environment.name().to_string(), generation.get() as i64],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .optional()?;
    decode_reservation(environment, row)
}

/// The most recent reservation for `environment`: a later generation is a new
/// activation, and the earlier row stays as the evidence a recovery reads.
fn latest_reservation(
    conn: &rusqlite::Connection,
    environment: &ResourceId,
) -> Result<Option<ActivationReservationRecord>, RunError> {
    let row = conn
        .query_row(
            "SELECT environment_generation, reservation_id, state, environment_run, \
             session_run, session_generation FROM workspace_activation_reservations \
             WHERE environment=?1 ORDER BY environment_generation DESC LIMIT 1",
            [environment.name().to_string()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                ))
            },
        )
        .optional()?;
    decode_reservation(environment, row)
}

fn activation_observe(cx: &EffectContext) -> Result<ObserveContext, RunError> {
    cx.operation().observe().map_err(|_| {
        RunError::broken("a workspace activation reservation needs an observing effect context")
    })
}

fn activation_reader(cx: &ObserveContext, root: &StateRoot) -> Result<StoreReader, RunError> {
    match Store::read(cx, root) {
        Observed::Verified(reader, _) => Ok(reader),
        Observed::Absent(_) => Err(RunError::broken("the kernel store is not readable")),
        Observed::Unknown(reason) => Err(RunError::unavailable(format!(
            "the kernel store is not readable: {reason}"
        ))),
    }
}

/// Whether a recorded run row is still open. The gate is not read: an
/// interrupted run is not live, and a successor settles it (K5) before it
/// stages a session of its own.
fn run_is_open(conn: &rusqlite::Connection, run: RunId) -> Result<bool, RunError> {
    let phase: Option<String> = conn
        .query_row(
            "SELECT phase FROM runs WHERE run_id=?1",
            [run.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(matches!(phase.as_deref(), Some(phase) if !matches!(phase, "stopping" | "closed")))
}

/// Whether a staged binding names a session run that is *live*: open at its
/// staged generation and holding its own owner gate. An interrupted run is not
/// live — its gate is free — so a successor settles it (K5) and stages its own
/// session, which is the recovery path. A liveness this kernel cannot establish
/// stays `Unknown`, and `Unknown` never displaces anything (R3).
fn binding_is_live(
    cx: &ObserveContext,
    root: &StateRoot,
    reader: &StoreReader,
    binding: &SessionBinding,
) -> Observed<bool> {
    match run_generation(reader.conn(), binding.run) {
        Ok(Some(generation)) if generation == binding.generation => {}
        Ok(_) => return Observed::Verified(false, activation_evidence()),
        Err(error) => return Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
    match reader.run_owner_state(cx, root, &binding.run) {
        Observed::Verified(OwnerState::Held, _) => Observed::Verified(true, activation_evidence()),
        Observed::Verified(OwnerState::Interrupted, _)
        | Observed::Verified(OwnerState::Closed, _)
        | Observed::Absent(_) => Observed::Verified(false, activation_evidence()),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

fn run_generation(conn: &rusqlite::Connection, run: RunId) -> Result<Option<Generation>, RunError> {
    let generation: Option<i64> = conn
        .query_row(
            "SELECT generation FROM runs WHERE run_id=?1",
            [run.to_string()],
            |row| row.get(0),
        )
        .optional()?;
    Ok(generation.map(|generation| Generation::new(generation as u64)))
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
    record: ActivationReservationRecord,
}

impl WorkspaceActivationReservation {
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
        crate::fault_point!(ReservationBefore)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let record = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            // The environment's row exists from its first reservation, exactly
            // as it exists from a run's admission: a first activation reserves
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
            let generation = Generation::new(current as u64)
                .next()
                .ok_or_else(|| RunError::refused("the environment generation cannot advance"))?;
            let existing = latest_reservation(txn.conn(), environment)?;
            match existing {
                Some(existing) if existing.generation > generation => Err(RunError::broken(
                    "a stored activation reservation is ahead of the environment generation",
                )),
                // Recovery, not a second reservation: the same id and E, with
                // whatever already bound them.
                Some(existing)
                    if existing.generation == generation
                        && existing.state != ActivationReservationState::Cancelled =>
                {
                    Ok(existing)
                }
                Some(existing) if existing.generation == generation => {
                    // A cancelled reservation whose generation no run ever took
                    // is revived: its id is derived from the same pair.
                    txn.conn().execute(
                        "UPDATE workspace_activation_reservations SET state='reserved',updated_at=?3 \
                         WHERE environment=?1 AND environment_generation=?2",
                        params![
                            environment.name().to_string(),
                            generation.get() as i64,
                            txn.now()
                        ],
                    )?;
                    Ok(ActivationReservationRecord {
                        state: ActivationReservationState::Reserved,
                        ..existing
                    })
                }
                _ => Self::insert(txn, environment, generation),
            }
        })?;
        crate::fault_point!(ReservationAfter)?;
        Ok(Self { record })
    }

    fn insert(
        txn: &mut Txn<'_>,
        environment: &ResourceId,
        generation: Generation,
    ) -> Result<ActivationReservationRecord, RunError> {
        let id = ActivationReservationId::derived(environment, generation);
        txn.conn().execute(
            "INSERT INTO workspace_activation_reservations \
             (environment, environment_generation, reservation_id, state, created_at, updated_at) \
             VALUES (?1,?2,?3,'reserved',?4,?4)",
            params![
                environment.name().to_string(),
                generation.get() as i64,
                id.to_string(),
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
        })
    }

    /// The most recent reservation `environment` recorded, whatever state it
    /// reached, so a caller that lost its handle recovers the same id and E.
    pub fn recover(
        reader: &StoreReader,
        environment: &ResourceId,
    ) -> Observed<Option<WorkspaceActivationReservation>> {
        match latest_reservation(reader.conn(), environment) {
            Ok(Some(record)) => Observed::Verified(Some(Self { record }), activation_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: RESERVATIONS.into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
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

    /// `claim(reservation_id, EnvRun)`: the claim a caller hands to
    /// [`Run::begin_claimed`], which binds this reservation to the environment
    /// activation run in that run's own admission transaction. The claim names
    /// no run yet, and admission refuses a reservation that already names
    /// another one, so a claim can bind exactly one activation run.
    pub fn claim(&self) -> Result<ActivationClaim, RunError> {
        match self.record.state {
            ActivationReservationState::Reserved => Ok(ActivationClaim {
                record: self.record.clone(),
            }),
            ActivationReservationState::Committed => {
                Err(ActivationRefusal::AlreadyCommitted.into())
            }
            ActivationReservationState::Cancelled => Err(ActivationRefusal::Cancelled.into()),
        }
    }

    /// `stage_session_binding(reservation, run, S)`: record that this
    /// Task-owned session run, at its own generation S, will serve as the
    /// environment's workspace session once E commits.
    ///
    /// Nothing is attached and no mapping is written: the environment is not
    /// active at E yet, and the publication receipt is validated against this
    /// staging before the commit writes anything. A live staged session is
    /// never displaced — a competing Task admission refuses here — while a
    /// staged session whose run settled or closed is replaced, which is the
    /// recovery path.
    pub fn stage_session_binding(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        session_run: &Run<'_>,
        session_generation: Generation,
    ) -> Result<(), RunError> {
        let fence = session_run.fence();
        if fence.generation != session_generation {
            return Err(RunError::refused(
                "the staged generation is not the session run's own owner generation",
            ));
        }
        check_session_owner(&self.record, &fence.resource)?;
        if matches!(session_run.phase()?, Phase::Stopping | Phase::Closed) {
            return Err(RunError::from(ActivationRefusal::SessionGone));
        }
        let binding = SessionBinding {
            run: session_run.id(),
            generation: session_generation,
        };
        // A live staged session is never displaced. Liveness is settled before
        // the write so the write itself carries no gate probe: the caller here
        // already holds this run's owner gate, so no competing live session can
        // appear between the probe and the transaction.
        let observe = activation_observe(cx)?;
        let reader = activation_reader(&observe, root)?;
        let stored = read_reservation(
            reader.conn(),
            &self.record.environment,
            self.record.generation,
        )?
        .ok_or_else(|| RunError::broken("the activation reservation vanished before staging"))?;
        if let Some(existing) = stored.session {
            if existing != binding {
                match binding_is_live(&observe, root, &reader, &existing) {
                    Observed::Verified(false, _) | Observed::Absent(_) => {}
                    Observed::Verified(true, _) => {
                        return Err(RunError::from(ActivationRefusal::SessionAlreadyStaged))
                    }
                    Observed::Unknown(reason) => {
                        return Err(RunError::unavailable(format!(
                            "the staged workspace session could not be observed: {reason}"
                        )))
                    }
                }
            }
        }
        crate::fault_point!(SessionStageBefore)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let stored = read_reservation(txn.conn(), &self.record.environment, self.record.generation)?
                .ok_or_else(|| RunError::broken("the activation reservation vanished before staging"))?;
            if stored.id != self.record.id {
                return Err(RunError::from(ActivationRefusal::WrongReservation));
            }
            match stored.state {
                ActivationReservationState::Reserved => {}
                ActivationReservationState::Committed => {
                    return Err(RunError::from(ActivationRefusal::AlreadyCommitted))
                }
                ActivationReservationState::Cancelled => {
                    return Err(RunError::from(ActivationRefusal::Cancelled))
                }
            }
            // The liveness that decided the replacement above is a gate
            // observation, so it is made outside this transaction. Here the
            // binding either already names this run or replaces a run that is
            // not live: the recovery path.
            if stored.session == Some(binding) {
                return Ok(());
            }
            let borrowed: bool = txn.conn().query_row(
                "SELECT EXISTS(SELECT 1 FROM attachments WHERE run_id=?1 AND state<>'released' AND environment<>?2)",
                params![
                    binding.run.to_string(),
                    self.record.environment.name().to_string()
                ],
                |row| row.get(0),
            )?;
            if borrowed {
                return Err(RunError::refused(
                    "the staged workspace session already borrows another environment",
                ));
            }
            txn.conn().execute(
                "UPDATE workspace_activation_reservations SET session_run=?3,session_generation=?4,\
                 updated_at=?5 WHERE environment=?1 AND environment_generation=?2",
                params![
                    self.record.environment.name().to_string(),
                    self.record.generation.get() as i64,
                    binding.run.to_string(),
                    binding.generation.get() as i64,
                    txn.now()
                ],
            )?;
            Ok::<_, RunError>(())
        })?;
        crate::fault_point!(SessionStageAfter)?;
        Ok(())
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
    /// crash before it leaves the reservation durable and the workspace
    /// unavailable: no mapping and no `active` state exist, so nothing can
    /// resolve the uncommitted session, and the next activation publishes a
    /// complete generation instead of replaying guest I/O.
    pub fn commit(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        environment_run: &Run<'_>,
    ) -> Result<CommittedActivation, RunError> {
        let fence = environment_run.fence().clone();
        if fence.resource != self.record.environment {
            return Err(RunError::from(ActivationRefusal::WrongEnvironment));
        }
        if fence.generation != self.record.generation {
            return Err(RunError::from(ActivationRefusal::WrongGeneration));
        }
        let receipt = published_receipt(environment_run)?;
        // The staged session must be live, not merely present: a mapping that
        // named a run whose gate is free would advertise a session no terminal
        // can attach to. Liveness is a gate observation, so it is made before
        // the commit's transaction; the transaction then re-validates the run's
        // row and generation with the rest of the plan.
        {
            let observe = activation_observe(cx)?;
            let reader = activation_reader(&observe, root)?;
            let stored = read_reservation(
                reader.conn(),
                &self.record.environment,
                self.record.generation,
            )?
            .ok_or_else(|| {
                RunError::broken("the activation reservation vanished before its commit")
            })?;
            let Some(binding) = stored.session else {
                return Err(RunError::from(ActivationRefusal::NotStaged));
            };
            match binding_is_live(&observe, root, &reader, &binding) {
                Observed::Verified(true, _) => {}
                Observed::Absent(_) => return Err(RunError::from(ActivationRefusal::SessionGone)),
                Observed::Verified(false, _) => {
                    return Err(RunError::from(ActivationRefusal::SessionGone))
                }
                Observed::Unknown(reason) => {
                    return Err(RunError::unavailable(format!(
                        "the staged workspace session could not be observed: {reason}"
                    )))
                }
            }
        }
        crate::fault_point!(ActivationCommitBefore)?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let committed = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            run::check_activation_fence(txn.conn(), &fence)?;
            let stored = read_reservation(txn.conn(), &self.record.environment, self.record.generation)?
                .ok_or_else(|| RunError::broken("the activation reservation vanished before its commit"))?;
            if stored.id != self.record.id {
                return Err(RunError::from(ActivationRefusal::WrongReservation));
            }
            let plan = activation_plan(txn.conn(), &stored)?
                .ok_or_else(|| RunError::from(ActivationRefusal::NotStaged))?;
            if plan.environment_run != Some(environment_run.id()) {
                return Err(RunError::from(ActivationRefusal::ClaimedByAnotherRun));
            }
            plan.validate_receipt(&receipt)?;
            if stored.state == ActivationReservationState::Committed {
                return Ok(CommittedActivation { plan });
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
            run::publish_active_state(txn, &fence)?;
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
            run::holder_attachment(txn, &fence, plan.session.run, AttachMode::Borrow)?;
            txn.conn().execute(
                "UPDATE workspace_activation_reservations SET state='committed',updated_at=?3 \
                 WHERE environment=?1 AND environment_generation=?2",
                params![
                    self.record.environment.name().to_string(),
                    self.record.generation.get() as i64,
                    txn.now()
                ],
            )?;
            Ok(CommittedActivation { plan })
        })?;
        crate::fault_point!(ActivationCommitAfter)?;
        Ok(committed)
    }

    /// Abandon this reservation before its commit. A live activation run at E is
    /// refused: its admission is durable and only its own settle ends it. A
    /// cancelled generation is never reused, so no later activation inherits
    /// the fence.
    pub fn cancel(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        wait: Wait,
    ) -> Result<Self, RunError> {
        let _gate = Gate::acquire(
            cx,
            root,
            std::slice::from_ref(&self.record.environment),
            wait,
        )?;
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let record = store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let stored =
                read_reservation(txn.conn(), &self.record.environment, self.record.generation)?
                    .ok_or_else(|| {
                        RunError::broken(
                            "the activation reservation vanished before it was cancelled",
                        )
                    })?;
            if stored.id != self.record.id {
                return Err(RunError::from(ActivationRefusal::WrongReservation));
            }
            match stored.state {
                ActivationReservationState::Committed => {
                    return Err(RunError::from(ActivationRefusal::AlreadyCommitted))
                }
                ActivationReservationState::Cancelled => return Ok(stored),
                ActivationReservationState::Reserved => {}
            }
            if let Some(run) = stored.environment_run {
                if run_is_open(txn.conn(), run)? {
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

    /// The check [`SessionRef::admit_reserved`] makes before it admits anything:
    /// a reservation that is not open, or that already has a live staged
    /// session, refuses the competing admission instead of settling it.
    fn admits_another_session(&self, cx: &EffectContext, root: &StateRoot) -> Result<(), RunError> {
        let observe = activation_observe(cx)?;
        let reader = activation_reader(&observe, root)?;
        // Read the durable record, not this handle's snapshot: another attempt
        // may have staged a session since the reservation was recovered.
        let Some(stored) = read_reservation(
            reader.conn(),
            &self.record.environment,
            self.record.generation,
        )?
        else {
            return Err(RunError::from(ActivationRefusal::Cancelled));
        };
        match stored.state {
            ActivationReservationState::Reserved => {}
            ActivationReservationState::Committed => {
                return Err(RunError::from(ActivationRefusal::AlreadyCommitted))
            }
            ActivationReservationState::Cancelled => {
                return Err(RunError::from(ActivationRefusal::Cancelled))
            }
        }
        let Some(binding) = stored.session else {
            return Ok(());
        };
        match binding_is_live(&observe, root, &reader, &binding) {
            Observed::Verified(false, _) => Ok(()),
            Observed::Verified(true, _) => {
                Err(RunError::from(ActivationRefusal::SessionAlreadyStaged))
            }
            Observed::Unknown(reason) => Err(RunError::unavailable(format!(
                "the staged workspace session could not be observed: {reason}"
            ))),
            Observed::Absent(_) => Ok(()),
        }
    }
}

/// The one right to admit an environment activation run at a reserved
/// generation E. Only [`WorkspaceActivationReservation::claim`] constructs one
/// and only [`Run::begin_claimed`] consumes it. It is not authority: admission
/// re-checks the durable reservation in its own transaction, so a claim for a
/// generation the environment no longer offers binds nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivationClaim {
    record: ActivationReservationRecord,
}

impl ActivationClaim {
    pub fn reservation_id(&self) -> ActivationReservationId {
        self.record.id
    }
    pub fn environment(&self) -> &ResourceId {
        &self.record.environment
    }
    /// E: the generation this claim admits at.
    pub fn generation(&self) -> Generation {
        self.record.generation
    }

    /// Bind the reservation to the run being admitted, inside the admission
    /// transaction. The generation is the one the environment counter advanced
    /// to, so a reservation that another activation consumed refuses here.
    pub(crate) fn bind(
        &self,
        txn: &mut Txn<'_>,
        owner: &ResourceId,
        generation: Generation,
        run: RunId,
    ) -> Result<(), RunError> {
        if owner.kind() != ResourceKind::Env || owner != &self.record.environment {
            return Err(RunError::from(ActivationRefusal::WrongEnvironment));
        }
        if generation != self.record.generation {
            return Err(RunError::from(ActivationRefusal::WrongGeneration));
        }
        let stored = read_reservation(txn.conn(), owner, generation)?.ok_or_else(|| {
            RunError::broken("the activation reservation vanished before its run was admitted")
        })?;
        if stored.id != self.record.id {
            return Err(RunError::from(ActivationRefusal::WrongReservation));
        }
        match stored.state {
            ActivationReservationState::Reserved => {}
            ActivationReservationState::Committed => {
                return Err(RunError::from(ActivationRefusal::AlreadyCommitted))
            }
            ActivationReservationState::Cancelled => {
                return Err(RunError::from(ActivationRefusal::Cancelled))
            }
        }
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
    let reserved: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM workspace_activation_reservations \
         WHERE environment=?1 AND environment_generation=?2 AND state='reserved')",
        params![owner.name().to_string(), generation.get() as i64],
        |row| row.get(0),
    )?;
    if reserved {
        return Err(RunError::refused(
            "this environment generation is reserved by a workspace activation; admit the activation through its reservation, or cancel the reservation first",
        ));
    }
    Ok(())
}

/// The guest publication an activation run recorded must be applied and carry
/// a receipt: `active` may only follow a completed required obligation (§8.11).
fn published_receipt(environment_run: &Run<'_>) -> Result<run::GuestPublicationReceipt, RunError> {
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
fn check_session_owner(
    record: &ActivationReservationRecord,
    owner: &ResourceId,
) -> Result<(), RunError> {
    if owner.kind() != ResourceKind::Task || owner == &record.environment {
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
    reservation: ActivationReservationId,
    environment: ResourceId,
    environment_generation: Generation,
    environment_run: Option<RunId>,
    session: SessionBinding,
    session_owner: ResourceId,
}

impl ActivationPlan {
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
                resource: self.session_owner.clone(),
                generation: self.session.generation,
            },
            self.environment.clone(),
            self.environment_generation,
        )
    }

    /// Validate an observed guest publication receipt against this plan. The
    /// plan is the staging, not a committed mapping: a receipt is validated
    /// before anything is published, so a crash after the immutable guest
    /// pointer but before the commit leaves the workspace unavailable rather
    /// than advertised.
    pub fn validate_receipt(&self, receipt: &run::GuestPublicationReceipt) -> Result<(), RunError> {
        if receipt.environment != self.fence() {
            return Err(ActivationRefusal::ReceiptMismatch.into());
        }
        if receipt.container_id.trim().is_empty()
            || receipt.vm_boot_id.trim().is_empty()
            || receipt.artifact_digest.trim().is_empty()
            || receipt.context_generation == 0
        {
            return Err(ActivationRefusal::ReceiptIncomplete.into());
        }
        Ok(())
    }
}

/// A committed activation: the plan it validated and the session identity every
/// front end now resolves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedActivation {
    plan: ActivationPlan,
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
fn activation_plan(
    conn: &rusqlite::Connection,
    record: &ActivationReservationRecord,
) -> Result<Option<ActivationPlan>, RunError> {
    let Some(session) = record.session else {
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
    check_session_owner(record, &session_owner)?;
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
    Ok(Some(ActivationPlan {
        reservation: record.id,
        environment: record.environment.clone(),
        environment_generation: record.generation,
        environment_run: record.environment_run,
        session,
        session_owner,
    }))
}
