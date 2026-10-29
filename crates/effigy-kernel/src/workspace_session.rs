//! The environment-owned workspace session (052 §8.7) and its client seam
//! (§8.11).
//!
//! An environment that declares an operator workspace has exactly one
//! `Detached` workspace session run. Its supervisor owns that session: it holds
//! the environment `Borrow` attachment, keeps the memory-only capabilities,
//! serves bounded terminal streams and coordinates a down requested from
//! inside. This module is the kernel half of that ownership — durable
//! request-keyed admission that survives a disconnect and a supervisor restart,
//! durable terminal attachment records, the bounded stream ring and the down
//! coordination — so a dispatcher can drive a session without inventing a
//! second authority. `crate::remote::{session, stream}` remains remote
//! *workload* custody: it proves and controls a workload the host already
//! admitted, and it is never host session authority.
//!
//! Nothing here spawns a process, binds a socket, signals a group or settles
//! another run. A child is admitted through `Run::begin_child`, control travels
//! as an ordinary journalled `run::request`, and a down takes its permit through
//! `run::Attachments`, so the session keeps exactly the authority it already
//! had.

mod admissions;
mod down;
mod grants;
mod streams;

#[cfg(test)]
mod tests;

pub use admissions::{
    Admission, AdmissionRecord, AdmissionRequest, AdmissionState, AdmissionTicket, Digest, Method,
    Refusal, RefusalCode, RequestId, ReserveDecision, SessionAdmissions, SessionKey, StoredOutcome,
    REQUEST_ID_BYTES,
};
pub use down::{DownChildren, DownCoordination, DownHolder, DownStart};
pub use grants::{
    host_entropy, owned, record_owned, refusal_category, terminals, AttachedTerminal, ClientGrant,
    Enrolment, OwnerLoss, SessionAuthority, SessionBinding, TerminalEnd, TerminalId, TerminalKind,
    TerminalState, SECRET_BYTES, TERMINAL_ID_BYTES,
};
pub use streams::{
    Frame, PushOutcome, ReadOutcome, StreamBuffer, StreamCursor, StreamError, StreamFrame,
    StreamId, StreamKind, MAX_FRAME_BYTES, MAX_QUEUED_BYTES, MAX_UNACKNOWLEDGED_FRAMES,
};

use crate::ctx::{CleanupContext, EffectContext, ObserveContext};
use crate::gate::Wait;
use crate::id::{Generation, ResolvedInvocation, ResourceId, RunId};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use crate::run::{
    self, AttachMode, Attachments, BeginOpts, Fence, Phase, RequestAction, Run, RunError,
    SettlementAdapter,
};
use crate::store::StoreReader;
use crate::supervise::Placement;
use rusqlite::{params, OptionalExtension};

/// The stored attachment mode of the one run that borrows the environment.
/// `AttachMode::Borrow` is its JSON encoding, so a session is the run that
/// borrows rather than a child run that holds the environment exclusively.
const BORROW_MODE: &str = "\"Borrow\"";

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
    /// What every front end prints for this session. One session, one label.
    pub fn label(&self) -> String {
        format!(
            "workspace session {} of {} (generation {})",
            self.run,
            self.environment.label(),
            self.generation().get()
        )
    }

    /// The session of `environment`: the non-closed run that borrows it. An
    /// environment has one workspace entry, and the entry's owner gate admits at
    /// most one unresolved run, so at most one such run exists.
    pub fn resolve(reader: &StoreReader, environment: &ResourceId) -> Observed<Option<SessionRef>> {
        let result = (|| -> Result<Option<SessionRef>, RunError> {
            let mut statement = reader.conn().prepare(
                "SELECT r.run_id FROM attachments a JOIN runs r ON r.run_id=a.run_id \
                 WHERE a.environment=?1 AND a.mode=?2 AND a.state<>'released' \
                 AND r.phase<>'closed' ORDER BY r.created_at DESC, r.run_id DESC LIMIT 1",
            )?;
            let Some(run) = statement
                .query_row(
                    params![environment.name().to_string(), BORROW_MODE],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
            else {
                return Ok(None);
            };
            let run = run
                .parse()
                .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?;
            match Self::of_run(reader, run) {
                Observed::Verified(session, _) => Ok(session),
                Observed::Absent(_) => Ok(None),
                Observed::Unknown(reason) => Err(RunError::unavailable(reason.to_string())),
            }
        })();
        match result {
            Ok(Some(session)) => Observed::Verified(Some(session), session_evidence()),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: "attachments".into(),
            }),
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// The session identity recorded for `run`, whatever attached it. A retry
    /// that found a recorded admission resolves its run this way, so the
    /// replayed answer names the same session the first attempt admitted.
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
    /// what a second entry finds and attaches to instead of duplicating, and
    /// the entry itself is guarded by the workspace entry's owner gate rather
    /// than by a request id: request ids only exist inside a session.
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
        let mut run = Run::begin(cx, root, invocation, opts, cleanup, adapter)?;
        Attachments::attach(&mut run, environment, AttachMode::Borrow, Wait::FailFast)?;
        let session = SessionRef::new(
            run.id(),
            run.fence().clone(),
            environment.resource.clone(),
            environment.generation,
        );
        Ok((run, session))
    }
}

/// A dispatcher-facing handle on one resolved session. It carries durable
/// identity and operations only: a second terminal holds this and never the
/// supervisor's owning pipe, so its lifetime is independent.
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

    /// Reserves a session request id before admission (§8.11). The reservation
    /// and the request digest commit together, so a concurrent or later retry
    /// reads one outcome instead of admitting a second run.
    pub fn reserve(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        request: &RequestId,
        admission: &AdmissionRequest,
    ) -> Result<ReserveDecision, RunError> {
        SessionAdmissions::reserve(cx, root, &self.session, request, admission)
    }

    /// Admits `opts` as a child of the session run: the session stays, the
    /// child settles its own workloads and attachment, and its parent link is
    /// the journal's record that it belonged to this session.
    ///
    /// The admission record names the child, so a replayed request returns that
    /// child instead of admitting another.
    #[allow(clippy::too_many_arguments)]
    pub fn admit_child<'a>(
        &self,
        cx: &EffectContext,
        root: &'a StateRoot,
        session_run: &mut Run<'a>,
        invocation: &ResolvedInvocation,
        opts: BeginOpts,
        cleanup: &CleanupContext,
        adapter: &mut impl SettlementAdapter,
        ticket: &AdmissionTicket,
    ) -> Result<Run<'a>, RunError> {
        if session_run.id() != self.session.run
            || session_run.fence() != &self.session.fence
            || ticket.run() != Some(opts.id)
        {
            return Err(RunError::broken(
                "a child admission names a different session or ticket",
            ));
        }
        let child = session_run.begin_child(invocation, opts, cleanup, adapter)?;
        if let Err(error) =
            SessionAdmissions::admitted(cx, root, ticket, &child, Placement::Detached)
        {
            child.settle(cleanup, adapter)?;
            return Err(error);
        }
        Ok(child)
    }

    /// Requests an ordinary stop of `target`, which must be this session or one
    /// of its child runs. The request is journalled against the target's current
    /// fence and the control socket wakes its owner; naming another client's run
    /// is refused before anything is written.
    ///
    /// Repeated with the same request id it is the same durable request, not a
    /// second one: a retried `cancel` answers with the run it already asked to
    /// stop, and the target still settles once.
    pub fn cancel(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        reader: &StoreReader,
        grant: &ClientGrant,
        request: &RequestId,
        target: RunId,
    ) -> Result<RunId, RunError> {
        grant.authorise_target(reader, &self.session, target)?;
        request_stop(cx, root, reader, request, target)
    }

    /// Stops one run that a terminal owned, after that terminal's EOF,
    /// disconnect or signal. The caller passes the request id it journalled and
    /// the [`OwnerLoss`] it observed; the stop is an ordinary journalled
    /// request, so a retried owner loss asks once and the run settles once. Only
    /// the runs [`SessionAuthority::owner_lost`] returned may be named here.
    pub fn stop_owned(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        reader: &StoreReader,
        request: &RequestId,
        run: RunId,
    ) -> Result<RunId, RunError> {
        if run == self.session.run {
            return Err(RunError::refused(
                "a terminal loss never stops the workspace session itself",
            ));
        }
        request_stop(cx, root, reader, request, run)
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
/// by a client's cancel and a session's down so both write the same primitive.
pub(crate) fn request_stop(
    cx: &EffectContext,
    root: &StateRoot,
    reader: &StoreReader,
    request: &RequestId,
    target: RunId,
) -> Result<RunId, RunError> {
    let fence = target_fence(reader, target)?;
    let stop = request.run_id()?;
    // A committed request is the durable fact: the same id naming the same stop
    // is that request, not a duplicate one. The journal refuses a second row, so
    // a retried cancel or owner loss answers instead of erroring.
    match recorded_request(reader, stop)? {
        Some(recorded) if recorded.run == target && recorded.expected == fence => {
            if !matches!(recorded.action, RequestAction::Stop) {
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
                action: RequestAction::Stop,
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
