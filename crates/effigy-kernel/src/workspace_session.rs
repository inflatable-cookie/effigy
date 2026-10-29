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

mod down;

#[cfg(test)]
mod tests;

pub use down::{DownChildren, DownCoordination, DownHolder, DownStart};

use crate::ctx::{CleanupContext, EffectContext, ObserveContext, StopCause};
use crate::gate::Wait;
use crate::id::{Generation, ResolvedInvocation, ResourceId, RunId};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use crate::run::{
    self, AttachMode, Attachments, BeginOpts, Fence, Phase, Release, Run, RunError,
    SettlementAdapter,
};
use crate::store::StoreReader;
use crate::supervise::Placement;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

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
        if placement != Placement::Detached {
            return Err(RunError::refused(
                "a workspace session is a Detached run, not an owning terminal",
            ));
        }
        // A `Detached` session is its own supervisor, so it plans that lifecycle
        // rather than promising one and running another.
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
