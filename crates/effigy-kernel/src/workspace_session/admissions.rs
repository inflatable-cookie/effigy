//! Durable request-keyed admission for the workspace session (§8.11).
//!
//! Every mutating session request is recorded before it is admitted, keyed by
//! the session generation and a 128-bit request id, together with the canonical
//! digest of the request. A concurrent or later retry with the same digest
//! reads the recorded run or outcome; a different digest is an `id_conflict`.
//! The record is durable, so it survives a disconnect and a supervisor restart,
//! and it is never silently forgotten and readmitted.
//!
//! The row also names the run the reservation will admit, derived from the
//! request identity rather than from a clock. Recovery therefore has one answer
//! for "did this request admit anything": the journal either holds that exact
//! run or it does not, and the reservation is read against it. When neither the
//! run nor a live session proves the admission, the answer is `unknown` — never
//! a second run.

use super::*;
use crate::ctx::Capability;
use crate::outcome::WorkloadOutcome;
use crate::store::{Store, Txn};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// The additive table this module owns (052 §8.11).
const TABLE: &str = "workspace_session_admissions";

/// Bytes in a request id: 128 bits, as §8.11 requires.
pub const REQUEST_ID_BYTES: usize = 16;
/// The longest recorded refusal or outcome label.
const TEXT_MAX: usize = 4096;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn bounded(text: &str, what: &str) -> Result<String, RunError> {
    if text.is_empty() || text.len() > TEXT_MAX {
        return Err(RunError::broken(format!(
            "a session {what} is out of bounds"
        )));
    }
    Ok(text.to_owned())
}

/// A session request method (§8.11). `observe` records no admission; every
/// other method is a Mutate that is recorded before it is dispatched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Observe,
    Invoke,
    Attach,
    Cancel,
    Detach,
}
impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Invoke => "invoke",
            Self::Attach => "attach",
            Self::Cancel => "cancel",
            Self::Detach => "detach",
        }
    }
    fn parse(value: &str) -> Result<Self, RunError> {
        match value {
            "observe" => Ok(Self::Observe),
            "invoke" => Ok(Self::Invoke),
            "attach" => Ok(Self::Attach),
            "cancel" => Ok(Self::Cancel),
            "detach" => Ok(Self::Detach),
            other => Err(RunError::broken(format!("a session method `{other}`"))),
        }
    }
    /// `observe` has no admission record and no side effects (§8.11).
    pub fn records_admission(self) -> bool {
        !matches!(self, Self::Observe)
    }
    /// Only `invoke` admits a run. `attach`, `cancel` and `detach` act on runs
    /// that already exist, so their recorded outcome is the operation's own
    /// receipt and they must never plan a run that will not exist.
    pub fn admits_a_run(self) -> bool {
        matches!(self, Self::Invoke)
    }
}

/// A 128-bit request id, unique per session generation, including across
/// reconnects.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RequestId(String);
impl RequestId {
    /// Parses the 32 lowercase hexadecimal characters §8.11 sends.
    pub fn parse(value: &str) -> Result<Self, RunError> {
        if value.len() != REQUEST_ID_BYTES * 2
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RunError::refused(
                "a request id is 32 lowercase hexadecimal characters",
            ));
        }
        Ok(Self(value.to_owned()))
    }
    /// Builds an id from entropy the host supplied. A request id grants no
    /// authority: it only makes a retry recognisable.
    pub fn generate(entropy: [u8; REQUEST_ID_BYTES]) -> Self {
        Self(hex(&entropy))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    /// The same value as a control request id, so a journalled stop names the
    /// session request that asked for it.
    pub fn run_id(&self) -> Result<RunId, RunError> {
        let bytes = (0..REQUEST_ID_BYTES)
            .map(|index| {
                u8::from_str_radix(&self.0[index * 2..index * 2 + 2], 16)
                    .map_err(|_| RunError::broken("a stored request id"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut millis = [0u8; 8];
        millis[2..].copy_from_slice(&bytes[..6]);
        let mut random = [0u8; 10];
        random.copy_from_slice(&bytes[6..]);
        RunId::new(u64::from_be_bytes(millis), random)
            .map_err(|error| RunError::broken(format!("a control request id ({error})")))
    }
}
impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The canonical digest of one session request: the method and the request
/// body, in the order the host canonicalises them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digest(String);
impl Digest {
    pub fn canonical(method: Method, body: &[u8]) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"effigy.workspace-session.request\0\x01");
        hash.update(method.as_str().as_bytes());
        hash.update([0u8]);
        hash.update(body);
        Self(hex(hash.finalize().as_slice()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The admission key: one session generation and one request id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionKey {
    generation: Generation,
    request: RequestId,
}
impl SessionKey {
    pub fn new(generation: Generation, request: RequestId) -> Self {
        Self {
            generation,
            request,
        }
    }
    pub fn of(session: &SessionRef, request: RequestId) -> Self {
        Self::new(session.generation(), request)
    }
    pub fn generation(&self) -> Generation {
        self.generation
    }
    pub fn request(&self) -> &RequestId {
        &self.request
    }
}

/// One request's method and digest, as the host recorded them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    method: Method,
    digest: Digest,
}
impl AdmissionRequest {
    pub fn new(method: Method, digest: Digest) -> Self {
        Self { method, digest }
    }
    pub fn canonical(method: Method, body: &[u8]) -> Self {
        Self::new(method, Digest::canonical(method, body))
    }
    pub fn method(&self) -> Method {
        self.method
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// A reservation this caller won: the session generation, request id, digest
/// and — for a method that admits a run — the run identity it will use.
#[derive(Debug, Clone)]
pub struct AdmissionTicket {
    key: SessionKey,
    digest: Digest,
    method: Method,
    run: Option<RunId>,
}
impl AdmissionTicket {
    /// The run an `invoke` reservation will admit. `attach`, `cancel` and
    /// `detach` plan no run, so they hold a ticket with none.
    pub fn run(&self) -> Option<RunId> {
        self.run
    }
    /// The run this reservation plans, when the method admits one.
    pub fn planned_run(&self) -> Option<RunId> {
        self.run
    }
    pub fn method(&self) -> Method {
        self.method
    }
    pub fn key(&self) -> &SessionKey {
        &self.key
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// What a completed admission recorded. Labels are the host's own outcome
/// words, never a channel-invented vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredOutcome {
    label: String,
    exit_code: Option<i32>,
}
impl StoredOutcome {
    pub fn new(label: impl Into<String>, exit_code: Option<i32>) -> Result<Self, RunError> {
        Ok(Self {
            label: bounded(&label.into(), "outcome label")?,
            exit_code,
        })
    }
    pub fn of(workload: &WorkloadOutcome, exit_code: i32) -> Self {
        let label = match workload {
            WorkloadOutcome::NotStarted => "not-started",
            WorkloadOutcome::Succeeded => "succeeded",
            WorkloadOutcome::Failed { .. } => "failed",
            WorkloadOutcome::Cancelled { .. } => "cancelled",
            WorkloadOutcome::TimedOut { .. } => "timed-out",
            WorkloadOutcome::Interrupted => "interrupted",
            WorkloadOutcome::Unknown(_) => "unknown",
        };
        Self {
            label: label.to_owned(),
            exit_code: Some(exit_code),
        }
    }
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }
}

/// The stable refusal codes §8.11 defines for this interface. The channel
/// renders them; nothing here invents a second vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefusalCode {
    InvalidRequest,
    Unauthenticated,
    Forbidden,
    StaleContext,
    IdentityMismatch,
    HeldBy,
    Unavailable,
    Unknown,
    IdConflict,
}
impl RefusalCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthenticated => "unauthenticated",
            Self::Forbidden => "forbidden",
            Self::StaleContext => "stale_context",
            Self::IdentityMismatch => "identity_mismatch",
            Self::HeldBy => "held_by",
            Self::Unavailable => "unavailable",
            Self::Unknown => "unknown",
            Self::IdConflict => "id_conflict",
        }
    }
}

/// A refusal with its stable code and a safe diagnostic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    code: RefusalCode,
    diagnostic: String,
}
impl Refusal {
    pub fn new(code: RefusalCode, diagnostic: impl Into<String>) -> Self {
        let diagnostic = diagnostic.into();
        Self {
            code,
            diagnostic: if diagnostic.len() > TEXT_MAX {
                effigy_ui::human::bound_text(&diagnostic, TEXT_MAX)
            } else {
                diagnostic
            },
        }
    }
    pub fn code(&self) -> RefusalCode {
        self.code
    }
    pub fn diagnostic(&self) -> &str {
        &self.diagnostic
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code.as_str(), self.diagnostic)
    }
}

/// The recorded state of one admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionState {
    /// The request id is reserved and nothing has been admitted yet.
    Reserved,
    /// The run exists. Its own journal owns what it is doing.
    Admitted,
    /// The run exists and the admission recorded its outcome.
    Completed,
    /// The request was refused and nothing ran.
    Refused,
}
impl AdmissionState {
    fn parse(value: &str) -> Result<Self, RunError> {
        match value {
            "reserved" => Ok(Self::Reserved),
            "admitted" => Ok(Self::Admitted),
            "completed" => Ok(Self::Completed),
            "refused" => Ok(Self::Refused),
            other => Err(RunError::broken(format!("a session admission `{other}`"))),
        }
    }
}

/// What a retry is told about a request id the host already holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// The recorded run. `phase` is read from the run's own journal when it is
    /// there, so a caller sees live or closed rather than guessing.
    Admitted {
        run: RunId,
        fence: Fence,
        placement: Placement,
        phase: Option<Phase>,
    },
    /// The recorded run and its recorded outcome.
    Completed {
        run: RunId,
        fence: Fence,
        placement: Placement,
        outcome: StoredOutcome,
    },
    /// A recorded non-run operation: `attach`, `cancel` or `detach` completed
    /// and its receipt is the outcome. No run exists to name.
    Applied { outcome: StoredOutcome },
    /// The recorded refusal. Nothing ran.
    Refused(Refusal),
    /// The request id is reserved by a session this journal proves live and has
    /// no outcome yet. A retry waits; it never admits or repeats the request.
    /// `planned_run` names the run an interrupted `invoke` will admit, and is
    /// `None` for a method that plans no run.
    Pending { planned_run: Option<RunId> },
    /// Recovery could not establish the admission or the outcome. A retry must
    /// report unknown; admitting a second run would be a second authority.
    Unknown { reason: String },
}

/// What `reserve` decided for one request.
#[derive(Debug, Clone)]
pub enum ReserveDecision {
    /// This caller holds the reservation and may admit exactly `ticket.run()`.
    Reserved(AdmissionTicket),
    /// The host already holds this request id.
    Replayed(Admission),
    /// The same id carries a different digest.
    Conflict { recorded: Digest },
    /// The session or environment generation is not current.
    Stale {
        expected: Generation,
        current: Option<Generation>,
    },
    /// The journal could not answer. Never a new run.
    Unknown { reason: String },
}
impl ReserveDecision {
    /// The refusal a response carries, or `None` when the decision itself is
    /// the answer (`Reserved` or `Replayed`).
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            Self::Reserved(_) | Self::Replayed(_) => None,
            Self::Conflict { recorded } => Some(Refusal::new(
                RefusalCode::IdConflict,
                format!(
                    "this request id was recorded with a different request (digest {})",
                    recorded.as_str()
                ),
            )),
            Self::Stale { expected, current } => Some(Refusal::new(
                RefusalCode::StaleContext,
                format!(
                    "the session expected generation {} but found {}",
                    expected.get(),
                    current.map_or_else(|| "none".to_owned(), |value| value.get().to_string())
                ),
            )),
            Self::Unknown { reason } => Some(Refusal::new(RefusalCode::Unknown, reason.clone())),
        }
    }
}

/// One recorded admission row, as a read-only reader sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRecord {
    key: SessionKey,
    digest: Digest,
    method: Method,
    state: AdmissionState,
    run: Option<RunId>,
    fence: Option<Fence>,
    placement: Option<Placement>,
    outcome: Option<StoredOutcome>,
    refusal: Option<Refusal>,
}
impl AdmissionRecord {
    pub fn key(&self) -> &SessionKey {
        &self.key
    }
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
    pub fn method(&self) -> Method {
        self.method
    }
    pub fn state(&self) -> AdmissionState {
        self.state
    }
    /// The run this record names, when the request admitted or planned one.
    /// A non-run operation names none, so a replay never invents a run.
    pub fn planned_run(&self) -> Option<RunId> {
        self.run
    }
    /// The recorded run when one exists.
    pub fn admitted_run(&self) -> Option<RunId> {
        self.run
    }
    pub fn placement(&self) -> Option<Placement> {
        self.placement
    }
    pub fn outcome(&self) -> Option<&StoredOutcome> {
        self.outcome.as_ref()
    }
    pub fn refusal(&self) -> Option<&Refusal> {
        self.refusal.as_ref()
    }
    /// What a retry is told, from the record alone.
    pub fn admission(&self) -> Admission {
        match self.state {
            AdmissionState::Reserved | AdmissionState::Admitted => {
                match (self.run, self.fence.clone(), self.placement) {
                    (Some(run), Some(fence), Some(placement)) => Admission::Admitted {
                        run,
                        fence,
                        placement,
                        phase: None,
                    },
                    _ => Admission::Pending {
                        planned_run: self.run,
                    },
                }
            }
            AdmissionState::Completed => match (self.run, self.fence.clone(), self.placement) {
                (Some(run), Some(fence), Some(placement)) => match &self.outcome {
                    Some(outcome) => Admission::Completed {
                        run,
                        fence,
                        placement,
                        outcome: outcome.clone(),
                    },
                    None => Admission::Admitted {
                        run,
                        fence,
                        placement,
                        phase: Some(Phase::Closed),
                    },
                },
                // A non-run operation's completed record is its receipt.
                (None, None, None) => match &self.outcome {
                    Some(outcome) => Admission::Applied {
                        outcome: outcome.clone(),
                    },
                    None => Admission::Unknown {
                        reason: "a completed non-run request records no outcome".to_owned(),
                    },
                },
                _ => Admission::Unknown {
                    reason: "a completed admission records an inconsistent run".to_owned(),
                },
            },
            AdmissionState::Refused => match &self.refusal {
                Some(refusal) => Admission::Refused(refusal.clone()),
                None => Admission::Unknown {
                    reason: "a refused admission records no refusal".to_owned(),
                },
            },
        }
    }
}

/// The admission store: one atomic reservation per (session generation,
/// request id).
#[derive(Debug)]
pub struct SessionAdmissions;

/// What one transaction concluded.
enum Considered {
    Stale {
        expected: Generation,
        current: Option<Generation>,
    },
    Row {
        fresh: bool,
        record: AdmissionRecord,
    },
}

impl SessionAdmissions {
    /// Reserves a request id before admission. Insert and read commit in one
    /// `BEGIN IMMEDIATE` transaction, so two callers cannot both win the same
    /// key and a concurrent retry reads exactly one recorded answer.
    pub fn reserve(
        cx: &EffectContext,
        root: &StateRoot,
        session: &SessionRef,
        request: &RequestId,
        admission: &AdmissionRequest,
    ) -> Result<ReserveDecision, RunError> {
        cx.require(Capability::AdapterEffect)
            .map_err(|_| RunError::refused("a session admission needs Mutate"))?;
        cx.require(Capability::HostAuthority).map_err(|_| {
            RunError::refused(
                "a session admission needs host authority, and this placement is a guest",
            )
        })?;
        if !admission.method().records_admission() {
            return Err(RunError::refused(
                "an observe request records no admission and makes no run",
            ));
        }
        let key = SessionKey::of(session, request.clone());
        let planned = if admission.method().admits_a_run() {
            Some(planned_run(session, &key, admission.digest())?)
        } else {
            None
        };
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        let considered = store.transaction(
            &cx.child(std::time::Duration::from_secs(30)),
            |txn| -> Result<Considered, RunError> {
                let environment = txn
                    .conn()
                    .query_row(
                        "SELECT generation FROM resources WHERE name=?1",
                        [session.environment.name().to_string()],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?;
                let current = environment.map(|value| Generation::new(value as u64));
                if current != Some(session.environment_generation) {
                    return Ok(Considered::Stale {
                        expected: session.environment_generation,
                        current,
                    });
                }
                let run = txn
                    .conn()
                    .query_row(
                        "SELECT generation, phase FROM runs WHERE run_id=?1",
                        [session.run.to_string()],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
                    )
                    .optional()?;
                let current = run
                    .as_ref()
                    .map(|(generation, _)| Generation::new(*generation as u64));
                let fresh = matches!(&run, Some((generation, phase))
                if *generation as u64 == session.generation().get() && phase != "closed");
                if !fresh {
                    return Ok(Considered::Stale {
                        expected: session.generation(),
                        current,
                    });
                }
                let inserted = txn.conn().execute(
                    "INSERT INTO workspace_session_admissions \
                 (session_generation, request_id, request_digest, method, state, run_id, \
                  created_at, updated_at) VALUES (?1, ?2, ?3, ?4, 'reserved', ?5, ?6, ?6) \
                 ON CONFLICT(session_generation, request_id) DO NOTHING",
                    params![
                        key.generation().get() as i64,
                        key.request().as_str(),
                        admission.digest().as_str(),
                        admission.method().as_str(),
                        planned.as_ref().map(ToString::to_string),
                        txn.now(),
                    ],
                )?;
                let record = read_row(txn, &key)?
                    .ok_or_else(|| RunError::broken("a session reservation vanished"))?;
                Ok(Considered::Row {
                    fresh: inserted == 1,
                    record,
                })
            },
        )?;
        match considered {
            Considered::Stale { expected, current } => {
                Ok(ReserveDecision::Stale { expected, current })
            }
            Considered::Row {
                fresh: true,
                record,
            } => Ok(ReserveDecision::Reserved(AdmissionTicket {
                key: record.key.clone(),
                digest: record.digest.clone(),
                method: record.method,
                run: planned,
            })),
            Considered::Row {
                fresh: false,
                record,
            } => Self::replay(cx, root, session, admission, record),
        }
    }

    /// Reads one recorded admission without admitting anything.
    pub fn lookup(reader: &StoreReader, key: &SessionKey) -> Observed<Option<AdmissionRecord>> {
        match read_reader(reader, key) {
            Ok(record) => match record {
                Some(record) => Observed::Verified(Some(record), session_evidence()),
                None => Observed::Absent(AbsenceProof::NoRow {
                    table: TABLE.into(),
                }),
            },
            Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
        }
    }

    /// Records that the reserved request admitted `run`. Idempotent: a repeated
    /// call for the same run changes nothing, and a completed record is never
    /// downgraded.
    pub fn admitted(
        cx: &EffectContext,
        root: &StateRoot,
        ticket: &AdmissionTicket,
        run: &Run<'_>,
        placement: Placement,
    ) -> Result<(), RunError> {
        if ticket.run != Some(run.id()) {
            return Err(RunError::broken(
                "an admission named a run other than its reservation",
            ));
        }
        let fence = run.fence().clone();
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let record = read_row(txn, &ticket.key)?
                .ok_or_else(|| RunError::broken("an admission reservation vanished"))?;
            if record.digest != ticket.digest {
                return Err(RunError::broken("an admission digest changed"));
            }
            match record.state {
                AdmissionState::Admitted | AdmissionState::Completed | AdmissionState::Refused => {
                    if record.run != Some(run.id()) {
                        return Err(RunError::broken("an admission run changed"));
                    }
                    return Ok(());
                }
                AdmissionState::Reserved => {}
            }
            txn.conn().execute(
                "UPDATE workspace_session_admissions SET state='admitted', run_id=?3, owner=?4, \
                 run_generation=?5, placement=?6, updated_at=?7 \
                 WHERE session_generation=?1 AND request_id=?2",
                params![
                    ticket.key.generation().get() as i64,
                    ticket.key.request().as_str(),
                    run.id().to_string(),
                    fence.resource.name().to_string(),
                    fence.generation.get() as i64,
                    placement.name(),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
    }

    /// Records the settled outcome of an admitted request.
    pub fn completed(
        cx: &EffectContext,
        root: &StateRoot,
        ticket: &AdmissionTicket,
        outcome: &StoredOutcome,
    ) -> Result<(), RunError> {
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let record = read_row(txn, &ticket.key)?
                .ok_or_else(|| RunError::broken("an admission reservation vanished"))?;
            if record.digest != ticket.digest || record.run != ticket.run {
                return Err(RunError::broken(
                    "an admission outcome names another request",
                ));
            }
            if record.state == AdmissionState::Completed {
                return Ok(());
            }
            if record.state != AdmissionState::Admitted {
                return Err(RunError::refused(
                    "an outcome needs an admitted run, not a reservation",
                ));
            }
            txn.conn().execute(
                "UPDATE workspace_session_admissions SET state='completed', outcome=?3, \
                 exit_code=?4, updated_at=?5 WHERE session_generation=?1 AND request_id=?2",
                params![
                    ticket.key.generation().get() as i64,
                    ticket.key.request().as_str(),
                    outcome.label(),
                    outcome.exit_code(),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
    }

    /// Records the receipt of a non-run operation (`attach`, `cancel`,
    /// `detach`). Its outcome is the operation's own answer — a terminal id or
    /// the run it stopped — so a retry reads the same answer instead of
    /// repeating the operation. Idempotent: a repeated record for the same
    /// ticket changes nothing.
    pub fn applied(
        cx: &EffectContext,
        root: &StateRoot,
        ticket: &AdmissionTicket,
        outcome: &StoredOutcome,
    ) -> Result<(), RunError> {
        if ticket.method.admits_a_run() {
            return Err(RunError::broken(
                "a run-admitting request records its run, not an applied receipt",
            ));
        }
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let record = read_row(txn, &ticket.key)?
                .ok_or_else(|| RunError::broken("an admission reservation vanished"))?;
            if record.digest != ticket.digest {
                return Err(RunError::broken(
                    "an admission receipt names another request",
                ));
            }
            match record.state {
                AdmissionState::Completed => return Ok(()),
                AdmissionState::Reserved => {}
                AdmissionState::Admitted | AdmissionState::Refused => {
                    return Err(RunError::refused(
                        "a receipt needs a reserved non-run request",
                    ))
                }
            }
            txn.conn().execute(
                "UPDATE workspace_session_admissions SET state='completed', outcome=?3, \
                 exit_code=?4, updated_at=?5 WHERE session_generation=?1 AND request_id=?2",
                params![
                    ticket.key.generation().get() as i64,
                    ticket.key.request().as_str(),
                    outcome.label(),
                    outcome.exit_code(),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
    }

    /// Records that the reserved request was refused before anything ran.
    pub fn refused(
        cx: &EffectContext,
        root: &StateRoot,
        ticket: &AdmissionTicket,
        refusal: &Refusal,
    ) -> Result<(), RunError> {
        let mut store = Store::open(&cx.child(std::time::Duration::from_secs(30)), root)?;
        store.transaction(&cx.child(std::time::Duration::from_secs(30)), |txn| {
            let record = read_row(txn, &ticket.key)?
                .ok_or_else(|| RunError::broken("an admission reservation vanished"))?;
            if record.digest != ticket.digest {
                return Err(RunError::broken(
                    "an admission refusal names another request",
                ));
            }
            if record.state != AdmissionState::Reserved {
                return Err(RunError::refused(
                    "a request that already ran cannot record a refusal",
                ));
            }
            txn.conn().execute(
                "UPDATE workspace_session_admissions SET state='refused', refusal_code=?3, \
                 refusal_detail=?4, updated_at=?5 WHERE session_generation=?1 AND request_id=?2",
                params![
                    ticket.key.generation().get() as i64,
                    ticket.key.request().as_str(),
                    refusal.code().as_str(),
                    refusal.diagnostic(),
                    txn.now(),
                ],
            )?;
            Ok::<_, RunError>(())
        })
    }

    /// The answer for a request id the host already holds.
    fn replay(
        cx: &EffectContext,
        root: &StateRoot,
        session: &SessionRef,
        admission: &AdmissionRequest,
        record: AdmissionRecord,
    ) -> Result<ReserveDecision, RunError> {
        if record.digest != *admission.digest() {
            return Ok(ReserveDecision::Conflict {
                recorded: record.digest.clone(),
            });
        }
        if record.state != AdmissionState::Reserved {
            return Ok(ReserveDecision::Replayed(record.admission()));
        }
        // A reservation is not proof that its run exists. Read the journal for
        // the planned run, then the session's own liveness; anything else is
        // unknown, never a fresh admission.
        let observe = match cx
            .operation()
            .child(std::time::Duration::from_secs(30))
            .observe()
        {
            Ok(observe) => observe,
            Err(_) => {
                return Ok(ReserveDecision::Unknown {
                    reason: "the journal could not be observed".to_owned(),
                })
            }
        };
        let reader = match Store::read(&observe, root) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(_) => {
                return Ok(ReserveDecision::Unknown {
                    reason: "the session journal is absent".to_owned(),
                })
            }
            Observed::Unknown(reason) => {
                return Ok(ReserveDecision::Unknown {
                    reason: reason.to_string(),
                })
            }
        };
        // A non-run request names no run, so a reservation of one is pending
        // while the session is live and does not consult the run journal.
        let planned = record.planned_run();
        if let Some(planned) = planned {
            match run::summary(&reader, planned) {
                Observed::Verified(summary, _) => {
                    let fence = Fence {
                        resource: session.fence.resource.clone(),
                        generation: summary.generation,
                    };
                    let placement = record.placement.unwrap_or(Placement::Detached);
                    return Ok(ReserveDecision::Replayed(Admission::Admitted {
                        run: planned,
                        fence,
                        placement,
                        phase: Some(summary.phase),
                    }));
                }
                Observed::Absent(_) => {}
                Observed::Unknown(reason) => {
                    return Ok(ReserveDecision::Unknown {
                        reason: reason.to_string(),
                    })
                }
            }
        }
        match reader.run_owner_state(&observe, root, &session.run) {
            Observed::Verified(crate::gate::OwnerState::Held, _) => {
                Ok(ReserveDecision::Replayed(Admission::Pending {
                    planned_run: planned,
                }))
            }
            Observed::Verified(_, _) => Ok(ReserveDecision::Unknown {
                reason: "the admitted session is not live, so its reservation cannot be resolved"
                    .to_owned(),
            }),
            Observed::Unknown(reason) => Ok(ReserveDecision::Unknown {
                reason: reason.to_string(),
            }),
            Observed::Absent(_) => Ok(ReserveDecision::Unknown {
                reason: "the admitted session has no owner gate".to_owned(),
            }),
        }
    }
}

/// The run identity a reservation will admit: derived from the session, the
/// request id and the digest, never from a clock, so recovery asks the journal
/// about one exact run.
fn planned_run(session: &SessionRef, key: &SessionKey, digest: &Digest) -> Result<RunId, RunError> {
    let mut hash = Sha256::new();
    hash.update(b"effigy.workspace-session.admission\0\x01");
    hash.update(session.run.bytes());
    hash.update(key.generation().get().to_be_bytes());
    hash.update(key.request().as_str().as_bytes());
    hash.update([0u8]);
    hash.update(digest.as_str().as_bytes());
    let bytes = hash.finalize();
    let mut millis = [0u8; 8];
    millis[2..].copy_from_slice(&bytes[..6]);
    let mut random = [0u8; 10];
    random.copy_from_slice(&bytes[6..16]);
    RunId::new(u64::from_be_bytes(millis), random)
        .map_err(|error| RunError::broken(format!("a planned run id ({error})")))
}

fn read_row(txn: &Txn<'_>, key: &SessionKey) -> Result<Option<AdmissionRecord>, RunError> {
    read_conn(txn.conn(), key)
}

fn read_reader(
    reader: &StoreReader,
    key: &SessionKey,
) -> Result<Option<AdmissionRecord>, RunError> {
    read_conn(reader.conn(), key)
}

fn read_conn(
    conn: &rusqlite::Connection,
    key: &SessionKey,
) -> Result<Option<AdmissionRecord>, RunError> {
    let row = conn
        .query_row(
            "SELECT request_digest, method, state, run_id, owner, run_generation, placement, \
             outcome, exit_code, refusal_code, refusal_detail FROM workspace_session_admissions \
             WHERE session_generation=?1 AND request_id=?2",
            params![key.generation().get() as i64, key.request().as_str()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<i32>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()?;
    let Some((
        digest,
        method,
        state,
        run,
        owner,
        run_generation,
        placement,
        outcome,
        exit_code,
        refusal_code,
        refusal_detail,
    )) = row
    else {
        return Ok(None);
    };
    let run = run
        .map(|run| {
            run.parse()
                .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))
        })
        .transpose()?;
    let fence = match (owner, run_generation) {
        (Some(owner), Some(generation)) => {
            let canonical = conn
                .query_row(
                    "SELECT canonical FROM resources WHERE name=?1",
                    [&owner],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?
                .ok_or_else(|| RunError::broken("an admission names an unknown owner"))?;
            Some(Fence {
                resource: decode_owner(&canonical)?,
                generation: Generation::new(generation as u64),
            })
        }
        _ => None,
    };
    let placement = placement
        .map(|value| match value.as_str() {
            "inline" => Ok(Placement::Inline),
            "detached" => Ok(Placement::Detached),
            "guest" => Ok(Placement::Guest),
            other => Err(RunError::broken(format!("a stored placement `{other}`"))),
        })
        .transpose()?;
    let refusal = match (refusal_code, refusal_detail) {
        (Some(code), Some(detail)) => Some(Refusal::new(
            match code.as_str() {
                "invalid_request" => RefusalCode::InvalidRequest,
                "unauthenticated" => RefusalCode::Unauthenticated,
                "forbidden" => RefusalCode::Forbidden,
                "stale_context" => RefusalCode::StaleContext,
                "identity_mismatch" => RefusalCode::IdentityMismatch,
                "held_by" => RefusalCode::HeldBy,
                "unavailable" => RefusalCode::Unavailable,
                "unknown" => RefusalCode::Unknown,
                "id_conflict" => RefusalCode::IdConflict,
                other => return Err(RunError::broken(format!("a stored refusal `{other}`"))),
            },
            detail,
        )),
        (Some(_), None) => return Err(RunError::broken("a stored refusal without a diagnostic")),
        (None, _) => None,
    };
    let outcome = outcome.map(|label| StoredOutcome { label, exit_code });
    Ok(Some(AdmissionRecord {
        key: key.clone(),
        digest: Digest(digest),
        method: Method::parse(&method)?,
        state: AdmissionState::parse(&state)?,
        run,
        fence,
        placement,
        outcome,
        refusal,
    }))
}
