//! The kernel half of the workspace channel (§8.11): replay-safe admission
//! and secondary attachment handles.
//!
//! The supervisor and the run journal remain the only admission and
//! settlement authority. This module adds two narrow journal records beside
//! them:
//!
//! - a keyed request row `(session, request ID)` carrying the canonical request
//!   digest, the one run it reserved and, once the run closes, that run's
//!   recorded outcome. A replay of the same key and digest recovers what was
//!   recorded; a changed digest is a conflict; a reservation whose admission
//!   cannot be established is `Unknown` and is never readmitted.
//! - a durable attachment row per client, so a second terminal has its own
//!   lifetime, bounded stream cursor and bounded control access without ever
//!   stopping, detaching or releasing the owning client's run.
//!
//! A session is its run *and* its generation: two sessions at the same
//! generation are different sessions and neither can name the other's run.
//! Neither record grants authority. The dispatcher has already checked the
//! session grant before it calls any of this; here a run that this session did
//! not admit, and a run whose journal is gone, are refused or `Unknown`.

use super::*;
use crate::ctx::{EffectContext, ObserveContext};
use crate::custody::Stream;
use crate::id::{Digest, Generation};
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::store::{Store, Txn};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

/// The most decoded bytes one stream read may return: §8.11's frame bound.
pub const MAX_CHUNK: usize = 32 * 1024;
/// The most terminals that may hold the same run at once.
pub const SECONDARY_MAX: usize = 8;
/// The most control requests that may wait on a run at once.
pub const CONTROL_MAX: usize = 4;

const ADMISSIONS: &str = "workspace_admissions";
const EVIDENCE: &str = "kernel workspace admission";

/// A 128-bit identity in §8.11's 32 lowercase hex characters. It names a
/// request or an attachment and grants nothing by being known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HexId([u8; 16]);
/// The text was not 32 lowercase hex characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidHexId;
impl std::fmt::Display for InvalidHexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("identity must be exactly 32 lowercase hex characters")
    }
}

pub use HexId as RequestId;
pub use HexId as AttachmentId;

impl HexId {
    pub fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
    pub fn parse(text: &str) -> Result<Self, InvalidHexId> {
        if text.len() != 32 {
            return Err(InvalidHexId);
        }
        let mut bytes = [0u8; 16];
        for (index, pair) in text.as_bytes().chunks(2).enumerate() {
            let high = nibble(pair[0]).ok_or(InvalidHexId)?;
            let low = nibble(pair[1]).ok_or(InvalidHexId)?;
            bytes[index] = (high << 4) | low;
        }
        Ok(Self(bytes))
    }
    /// First 128 bits of the digest of `material`, for an id this module owns.
    fn derived(material: &[u8]) -> Self {
        let digest = Digest::of(material);
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.0[..16]);
        Self(bytes)
    }
    pub fn bytes(&self) -> &[u8; 16] {
        &self.0
    }
}
fn nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}
impl std::fmt::Display for HexId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}
impl Serialize for HexId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}
impl<'de> Deserialize<'de> for HexId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// One workspace session, as the grant binds it: the session's own run and its
/// generation. Two sessions that share a generation are still different
/// sessions, so neither may name the other's run (§8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Session {
    pub run: RunId,
    pub generation: Generation,
}

/// One mutating request in one workspace session. Request ids are unique per
/// session, including across reconnects, so this key names at most one admitted
/// run for the life of the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AdmissionKey {
    pub session: Session,
    pub request: RequestId,
}

/// What a keyed request finally produced. The kernel records it in the same
/// transaction that closes the run, so the answer survives the run's own
/// pruning and a later replay is answered, never forgotten.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordedOutcome {
    pub workload: WorkloadOutcome,
    pub cleanup_complete: bool,
}

/// Why a recorded request cannot be answered as an admission or an outcome.
/// Both are answers; neither is permission to admit again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnknownAdmission {
    /// A reservation committed but no admission can be established for it. The
    /// run id it was reserved with is not reported: no journal row ever bound
    /// it to this request, so claiming it would attribute another run.
    ReservedNotAdmitted,
    /// The run's journal or live owner cannot be recovered and no outcome is known.
    AdmissionNotRecovered,
}
impl std::fmt::Display for UnknownAdmission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReservedNotAdmitted => f.write_str(
                "the request was reserved and no admission or outcome can be established for it",
            ),
            Self::AdmissionNotRecovered => f.write_str(
                "the request's journal or live owner cannot be established and no outcome is known",
            ),
        }
    }
}

/// The recorded answer for a keyed request.
#[derive(Debug, Clone, PartialEq)]
pub enum Recovered {
    /// The run is admitted and still in its journal. `fence` is the owner
    /// generation a control request must present.
    Admitted { run: RunId, fence: Fence },
    /// The run closed; this outcome is the answer for the key from now on.
    Completed {
        run: RunId,
        outcome: RecordedOutcome,
    },
    /// The record exists and cannot be resolved to an admission or an outcome.
    Unknown { reason: UnknownAdmission },
    /// The key already names a request with a different canonical digest.
    Conflict { recorded: Digest },
}

/// What `reserve` found for its key.
#[derive(Debug, PartialEq)]
pub enum Reservation {
    /// This caller won the one reservation and must admit `ticket`'s run.
    Fresh(AdmissionTicket),
    /// The key is already reserved, admitted or closed: recover it, never
    /// launch a second run for it.
    Recorded(Recovered),
}

/// The one right to admit a reserved key. Only `reserve` constructs one, and
/// only [`Run::begin_admitted`] consumes it. It is transferable — a host
/// dispatcher hands it to a detached engine supervisor through
/// [`crate::supervise::EngineLaunch::admission`] — but it is not authority:
/// admission re-checks the durable reservation in its own transaction, so a
/// ticket for a key that is not reserved for that run binds nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdmissionTicket {
    key: AdmissionKey,
    run: RunId,
    digest: Digest,
}
impl AdmissionTicket {
    pub fn key(&self) -> AdmissionKey {
        self.key
    }
    pub fn run(&self) -> RunId {
        self.run
    }
    /// The canonical request digest this reservation was recorded with. A
    /// replay must present the same one.
    pub fn digest(&self) -> &Digest {
        &self.digest
    }
}

/// Reserve a keyed mutating request before any supervisor is launched
/// (§8.11). The reservation is durable and idempotent: one caller gets
/// `Fresh`, every other caller — concurrent, later or after a restart — gets
/// what is already recorded for the key.
pub fn reserve(
    cx: &EffectContext,
    root: &StateRoot,
    key: AdmissionKey,
    digest: Digest,
    run: RunId,
) -> Result<Reservation, RunError> {
    mutations_only(cx, "a workspace admission")?;
    // An engine's workload may be open-ended; this journal phase, including
    // replay's owner observation, must still have one finite deadline.
    let cx = &cx.child(std::time::Duration::from_secs(30));
    let mut store = Store::open_within(cx, root, BusyWait::Context)?;
    crate::fault_point!(AdmissionReserveBefore)?;
    let recorded = store.transaction_within(cx, BusyWait::Context, |txn| {
        insert_reservation(txn.conn(), txn.now(), key, &digest, run, cx, root)
    })?;
    crate::fault_point!(AdmissionReserveAfter)?;
    Ok(match recorded {
        None => Reservation::Fresh(AdmissionTicket { key, run, digest }),
        Some(recorded) => Reservation::Recorded(recorded),
    })
}

/// The one insert-and-classify step. Callers hold the store's writer lock, so
/// two concurrent identical requests cannot both see an empty key, and a run
/// identity can never be claimed by two keys.
fn insert_reservation(
    conn: &Connection,
    now: i64,
    key: AdmissionKey,
    digest: &Digest,
    run: RunId,
    cx: &EffectContext,
    root: &StateRoot,
) -> Result<Option<Recovered>, RunError> {
    // A key that already has a record is a replay, whatever run id it offers
    // this time: classify it before looking at the proposed identity, so a
    // same-key retry recovers its admission (or reports its digest conflict)
    // even when its fresh entropy lands on an id another key holds.
    if let Some(row) = row(conn, &key)? {
        return replayed(
            conn,
            &row,
            key,
            digest,
            &cx.operation()
                .observe()
                .map_err(|_| RunError::refused("observation required"))?,
            root,
        )
        .map(Some);
    }
    let session_owner = current_session(conn, key.session, None)?;
    // A genuinely new key takes a run identity that no other key holds and the
    // journal does not already contain. Refusing here rolls the reservation
    // back, because the caller holds the store's writer lock.
    if admission_holder(conn, run)?.is_some() {
        return Err(RunError::refused(
            "this run id already belongs to another workspace request",
        ));
    }
    let taken: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM runs WHERE run_id=?1)",
        [run.to_string()],
        |r| r.get(0),
    )?;
    if taken {
        return Err(RunError::refused("this run id already names a run"));
    }
    conn.execute(
        "INSERT INTO workspace_admissions(session_id,session_generation,request_id,digest,run_id,state,created_at,updated_at,session_owner) \
         VALUES (?1,?2,?3,?4,?5,'reserved',?6,?6,?7)",
        params![
            key.session.run.to_string(),
            key.session.generation.get() as i64,
            key.request.to_string(),
            digest.0.as_slice(),
            run.to_string(),
            now,
            session_owner
        ],
    )?;
    Ok(None)
}

/// Read a keyed request without writing anything. `Absent` means nothing was
/// recorded for the key, so a fresh reservation is the only way forward.
pub fn lookup(
    cx: &ObserveContext,
    root: &StateRoot,
    key: AdmissionKey,
    digest: Digest,
) -> Observed<Recovered> {
    #[cfg(all(test, feature = "fault-injection"))]
    if let Some(injected) = crate::fault::injected() {
        return injected;
    }
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let result = (|| {
        current_session(reader.conn(), key.session, None)?;
        let Some(row) = row(reader.conn(), &key)? else {
            return Ok(None);
        };
        replayed(reader.conn(), &row, key, &digest, cx, root).map(Some)
    })();
    match result {
        Ok(Some(recovered)) => Observed::Verified(
            recovered,
            Evidence::new(EVIDENCE.into(), None).expect("fixed evidence"),
        ),
        Ok(None) => Observed::Absent(AbsenceProof::NoRow {
            table: ADMISSIONS.into(),
        }),
        Err(error) => Observed::Unknown(UnknownReason::StoreUnreadable(error.to_string())),
    }
}

/// Whether a request with this key was ever recorded, for a reconnect that
/// must report `unknown` rather than admit again.
fn replayed(
    conn: &Connection,
    row: &Row,
    key: AdmissionKey,
    offered: &Digest,
    cx: &ObserveContext,
    root: &StateRoot,
) -> Result<Recovered, RunError> {
    if row.digest != *offered {
        return Ok(Recovered::Conflict {
            recorded: row.digest.clone(),
        });
    }
    current_session(
        conn,
        key.session,
        Some(
            row.session_owner
                .as_deref()
                .ok_or_else(|| RunError::refused("the recorded session owner is unknown"))?,
        ),
    )?;
    let journal = journal_run(conn, row.run)?;
    match (row.state, journal) {
        // A reservation whose admission never committed is never readmitted,
        // and never names a run: the journal cannot prove what the killed
        // client was about to do, or that the id it offered was its own.
        (State::Reserved, _) => Ok(Recovered::Unknown {
            reason: UnknownAdmission::ReservedNotAdmitted,
        }),
        (State::Closed, _) => match &row.outcome {
            Some(outcome) => Ok(Recovered::Completed {
                run: row.run,
                outcome: outcome.clone(),
            }),
            None => Ok(Recovered::Unknown {
                reason: UnknownAdmission::AdmissionNotRecovered,
            }),
        },
        (State::Admitted, Some(record)) if record.phase == Phase::Closed => {
            let workload = record
                .workload
                .ok_or_else(|| RunError::broken("a closed run recorded no workload outcome"))?;
            Ok(Recovered::Completed {
                run: row.run,
                outcome: RecordedOutcome {
                    workload,
                    cleanup_complete: record.cleanup.as_deref() == Some("complete"),
                },
            })
        }
        // Only the admission transaction moves a reservation to `admitted`,
        // and it does so with the run row it had just inserted, so this run is
        // the one this key bound.
        (State::Admitted, Some(record)) => {
            if live_owner(cx, root, conn, row.run, &record).is_err() {
                return Ok(Recovered::Unknown {
                    reason: UnknownAdmission::AdmissionNotRecovered,
                });
            }
            Ok(Recovered::Admitted {
                run: row.run,
                fence: fence(conn, &record.owner, record.generation)?,
            })
        }
        (_, None) => Ok(Recovered::Unknown {
            reason: UnknownAdmission::AdmissionNotRecovered,
        }),
    }
}

/// Bind a reservation to its run inside the admission transaction, so the
/// run row, the key's `admitted` state and the owning client's attachment
/// commit together or not at all.
pub(super) fn bind(txn: &mut Txn<'_>, ticket: &AdmissionTicket) -> Result<(), RunError> {
    let row = row(txn.conn(), &ticket.key)?.ok_or_else(|| RunError::refused("no reservation"))?;
    current_session(
        txn.conn(),
        ticket.key.session,
        Some(
            row.session_owner
                .as_deref()
                .ok_or_else(|| RunError::refused("the recorded session owner is unknown"))?,
        ),
    )?;
    let claimed = txn.conn().execute(
        "UPDATE workspace_admissions SET state='admitted',updated_at=?6 \
         WHERE session_id=?1 AND session_generation=?2 AND request_id=?3 AND run_id=?4 AND digest=?5 AND state='reserved'",
        params![
            ticket.key.session.run.to_string(),
            ticket.key.session.generation.get() as i64,
            ticket.key.request.to_string(),
            ticket.run.to_string(),
            ticket.digest.0.as_slice(),
            txn.now()
        ],
    )?;
    if claimed != 1 {
        return Err(RunError::broken(
            "an admission ticket does not match its reservation",
        ));
    }
    // The owning client's lifetime begins with its admission. Only it may
    // release this row; a second terminal never can (§8.11).
    txn.conn().execute(
        "INSERT INTO run_attachments(attachment_id,run_id,session_id,session_generation,lifetime,state,created_at) \
         VALUES (?1,?2,?3,?4,'owner','attached',?5)",
        params![
            owner_attachment(ticket.run).to_string(),
            ticket.run.to_string(),
            ticket.key.session.run.to_string(),
            ticket.key.session.generation.get() as i64,
            txn.now()
        ],
    )?;
    Ok(())
}

/// A reserved run identity belongs to its key. An ordinary run that offers the
/// same id is refused
/// inside its admission transaction, so a reservation can never be swapped for
/// another workload (§8.11).
pub(super) fn refuse_unkeyed(txn: &Txn<'_>, run: RunId) -> Result<(), RunError> {
    let reserved: bool = txn.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM workspace_admissions WHERE run_id=?1)",
        [run.to_string()],
        |r| r.get(0),
    )?;
    if reserved {
        return Err(RunError::refused(
            "this run id is reserved for a keyed workspace request",
        ));
    }
    Ok(())
}

/// The run's close transaction records the keyed request's final outcome and
/// ends every client attachment in the same commit as `phase='closed'`.
pub(super) fn closed(
    txn: &Txn<'_>,
    run: RunId,
    workload: &WorkloadOutcome,
    cleanup_complete: bool,
) -> Result<(), RunError> {
    let outcome = serde_json::to_string(&RecordedOutcome {
        workload: workload.clone(),
        cleanup_complete,
    })?;
    // Only the binding that this run actually admitted records an outcome. An
    // unadmitted reservation with the same run id stays `reserved`, so its key
    // keeps answering Unknown instead of claiming another workload's outcome.
    txn.conn().execute(
        "UPDATE workspace_admissions SET state='closed',outcome=?2,updated_at=?3 \
         WHERE run_id=?1 AND state='admitted'",
        params![run.to_string(), outcome, txn.now()],
    )?;
    txn.conn().execute(
        "DELETE FROM run_attachments WHERE run_id=?1",
        [run.to_string()],
    )?;
    Ok(())
}

/// Which client an attachment belongs to. Only the owner's EOF settles its
/// run; a secondary terminal has a lifetime of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Client {
    Owner,
    Secondary,
}

/// How an attachment ended. A detach is acknowledged before the client goes;
/// an EOF is the client going away without one (§8.11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Release {
    Eof,
    Detach,
}

/// One client's attachment to an admitted run: a bounded stream cursor and a
/// bounded control path, and nothing else. Every read and control re-checks the
/// durable attachment row, so a released attachment, a run that closed, or a
/// row that a newer attach replaced cannot keep reading or steering. Releasing
/// a secondary never touches the run; owner EOF requests stop.
#[derive(Debug)]
pub struct Attachment {
    id: AttachmentId,
    run: RunId,
    fence: Fence,
    session: Session,
    client: Client,
    /// Absolute byte position per `(process sequence, stream)`.
    cursors: BTreeMap<(i64, &'static str), u64>,
    released: bool,
}
impl Attachment {
    /// Supervisor liveness needs only the identity, never a terminal's cursors.
    pub(crate) fn lifetime_handle(&self) -> Self {
        Self {
            id: self.id,
            run: self.run,
            fence: self.fence.clone(),
            session: self.session,
            client: self.client,
            cursors: BTreeMap::new(),
            released: self.released,
        }
    }
    pub fn id(&self) -> AttachmentId {
        self.id
    }
    pub fn run(&self) -> RunId {
        self.run
    }
    pub fn fence(&self) -> &Fence {
        &self.fence
    }
    pub fn session(&self) -> Session {
        self.session
    }
    pub fn client(&self) -> Client {
        self.client
    }
    /// Whether this handle has released its own lifetime. A durable release by
    /// another holder is reported by `attached`, not here.
    pub fn released(&self) -> bool {
        self.released
    }
    /// Whether this attachment is still attached in the journal.
    pub fn attached(&self, cx: &EffectContext, root: &StateRoot) -> Result<bool, RunError> {
        let cx = cx.child(std::time::Duration::from_secs(30));
        let observed = cx
            .operation()
            .observe()
            .map_err(|_| RunError::refused("this context cannot observe an attachment"))?;
        self.attached_now(&observed, root)
    }
    fn attached_now(&self, cx: &ObserveContext, root: &StateRoot) -> Result<bool, RunError> {
        match Store::read(cx, root) {
            Observed::Verified(reader, _) => {
                if !attached_row(reader.conn(), self.id, self)? {
                    return Ok(false);
                }
                require_live_owner(cx, root, reader.conn(), self.run)?;
                Ok(true)
            }
            Observed::Absent(_) => Ok(false),
            Observed::Unknown(reason) => Err(RunError::unavailable(format!(
                "this attachment's lifetime could not be read ({reason})"
            ))),
        }
    }
    /// The next byte this attachment will read from one process stream.
    pub fn cursor(&self, sequence: i64, stream: Stream) -> u64 {
        self.cursors
            .get(&(sequence, stream.name()))
            .copied()
            .unwrap_or(0)
    }
    /// Read at most `max` bytes from where this attachment last stopped on
    /// `sequence`'s `stream`. A cursor the log has rotated away is reported as
    /// a `gap`, never presented as complete output (§8.11).
    pub fn read(
        &mut self,
        cx: &ObserveContext,
        root: &StateRoot,
        sequence: i64,
        stream: Stream,
        max: usize,
    ) -> Result<Chunk, RunError> {
        if sequence < 0 {
            return Err(RunError::refused("a stream sequence is not negative"));
        }
        if max == 0 || max > MAX_CHUNK {
            return Err(RunError::refused(format!(
                "a stream read is between 1 and {MAX_CHUNK} bytes"
            )));
        }
        // The terminal's own lifetime, not a remembered flag: a released
        // attachment, or one whose run closed and took its row, stops here.
        self.lifetime(cx, root)?;
        let offset = self.cursor(sequence, stream);
        let read = match read_log(cx, root, self.run, sequence, stream, offset, max) {
            Observed::Verified(read, _) => read,
            Observed::Absent(_) => {
                return Err(RunError::refused("this run kept no log for that stream"))
            }
            Observed::Unknown(reason) => {
                return Err(RunError::unavailable(format!(
                    "the run's captured output could not be read ({reason})"
                )))
            }
        };
        if offset > read.end {
            return Err(RunError::refused(
                "this attachment's cursor is ahead of the captured stream",
            ));
        }
        let next = read.from + read.bytes.len() as u64;
        self.cursors.insert((sequence, stream.name()), next);
        let gap = (read.from > offset).then_some(Gap {
            from: offset,
            to: read.from,
        });
        Ok(Chunk {
            sequence,
            stream,
            from: read.from,
            next,
            end: read.end,
            bytes: read.bytes,
            gap,
            truncated: read.truncated,
        })
    }
    /// Record one bounded stop or escalation against this run's current owner
    /// generation. The dispatcher's grant check has already authorised it; this
    /// refuses the actions that belong to the owning client instead. The bound
    /// and the insert commit in one transaction, so concurrent controls cannot
    /// exceed [`CONTROL_MAX`] between them.
    pub fn control(
        &self,
        cx: &EffectContext,
        root: &StateRoot,
        id: RunId,
        action: RequestAction,
    ) -> Result<(), RunError> {
        if !matches!(action, RequestAction::Stop | RequestAction::Escalate) {
            return Err(RunError::refused(
                "a stop or escalation is this attachment's only control; a detach is its own \
                 release and settlement belongs to the run",
            ));
        }
        if self.released {
            return Err(RunError::refused("this attachment was already released"));
        }
        // The attachment row is checked inside the transaction that inserts the
        // request, so a release that lands between this call and that commit
        // cannot let a control through after the terminal is gone.
        request_bounded(
            cx,
            root,
            &Request {
                id,
                run: self.run,
                expected: self.fence.clone(),
                action,
            },
            Some(CONTROL_MAX),
            Some(self),
        )
    }
    /// Release this client's lifetime, recording an acknowledged detach apart
    /// from an EOF. An owning client's EOF records the stop its run needs
    /// (§8.11) in the same commit; an acknowledged detach leaves the run going,
    /// and a secondary terminal going away never stops, settles or detaches the
    /// owning client's run.
    pub fn release(
        &mut self,
        cx: &EffectContext,
        root: &StateRoot,
        cause: Release,
    ) -> Result<(), RunError> {
        if self.released {
            return Ok(());
        }
        release_attachment(cx, root, self, cause)?;
        self.released = true;
        Ok(())
    }
    /// This handle's own durable lifetime: its row must still be attached to
    /// this run in this session.
    fn lifetime(&self, cx: &ObserveContext, root: &StateRoot) -> Result<(), RunError> {
        if self.released || !self.attached_now(cx, root)? {
            return Err(RunError::refused(
                "this attachment is no longer attached to the run",
            ));
        }
        Ok(())
    }
}

/// One bounded read of a captured stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Chunk {
    pub sequence: i64,
    pub stream: Stream,
    /// Offset of the first returned byte: greater than the requested cursor
    /// when rotation removed the bytes in between.
    pub from: u64,
    /// The next byte this attachment reads.
    pub next: u64,
    /// The stream's end offset when read.
    pub end: u64,
    pub bytes: Vec<u8>,
    pub gap: Option<Gap>,
    pub truncated: bool,
}

/// Bytes the sender no longer has. The client obtains current status or logs
/// from the host; a gap is never complete output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Gap {
    pub from: u64,
    pub to: u64,
}

/// Re-open the owning client's handle on a run this session admitted. A
/// reconnect after a supervisor restart takes exactly this path; it never
/// creates an owner and never takes one from another session.
pub fn resume_owner(
    cx: &EffectContext,
    root: &StateRoot,
    session: Session,
    run: RunId,
) -> Result<Attachment, RunError> {
    open(cx, root, session, run, Client::Owner, None)
}

/// Attach a second terminal to a run this session admitted. Its lifetime,
/// cursor and control path are its own; the owning client's attachment is
/// untouched.
pub fn attach_secondary(
    cx: &EffectContext,
    root: &StateRoot,
    session: Session,
    run: RunId,
    id: AttachmentId,
) -> Result<Attachment, RunError> {
    open(cx, root, session, run, Client::Secondary, Some(id))
}

fn open(
    cx: &EffectContext,
    root: &StateRoot,
    session: Session,
    run: RunId,
    client: Client,
    id: Option<AttachmentId>,
) -> Result<Attachment, RunError> {
    mutations_only(cx, "a workspace attachment")?;
    let cx = &cx.child(std::time::Duration::from_secs(30));
    let mut store = Store::open_within(cx, root, BusyWait::Context)?;
    crate::fault_point!(RunAttachBefore)?;
    let (id, fence) = store.transaction_within(cx, BusyWait::Context, |txn| {
        let conn = txn.conn();
        let admitted = admission_session(conn, run)?
            .ok_or_else(|| RunError::refused("this run was not admitted by a session"))?;
        if admitted != session {
            return Err(RunError::refused(
                "this run was admitted in another workspace session",
            ));
        }
        let record = journal_run(conn, run)?
            .ok_or_else(|| RunError::refused("this run is no longer in its journal"))?;
        if record.phase == Phase::Closed {
            return Err(RunError::refused("this run is closed"));
        }
        let fence = fence(conn, &record.owner, record.generation)?;
        journal::check_fence(conn, &fence)?;
        live_owner(&cx.operation().observe().map_err(|_| RunError::refused("observation required"))?, root, conn, run, &record)?;
        match client {
            Client::Owner => {
                let attached: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM run_attachments WHERE run_id=?1 AND lifetime='owner' AND state='attached')",
                    [run.to_string()],
                    |r| r.get(0),
                )?;
                if !attached {
                    return Err(RunError::refused(
                        "this run has no attached owning client",
                    ));
                }
                Ok((owner_attachment(run), fence))
            }
            Client::Secondary => {
                let id = id.ok_or_else(|| RunError::broken("a secondary needs its id"))?;
                let attached: i64 = conn.query_row(
                    "SELECT count(*) FROM run_attachments WHERE run_id=?1 AND lifetime='secondary' AND state='attached'",
                    [run.to_string()],
                    |r| r.get(0),
                )?;
                if attached >= SECONDARY_MAX as i64 {
                    return Err(RunError::refused(format!(
                        "this run already has {SECONDARY_MAX} attached terminals"
                    )));
                }
                let inserted = conn.execute(
                    "INSERT INTO run_attachments(attachment_id,run_id,session_id,session_generation,lifetime,state,created_at) \
                     VALUES (?1,?2,?3,?4,'secondary','attached',?5) ON CONFLICT(attachment_id) DO NOTHING",
                    params![
                        id.to_string(),
                        run.to_string(),
                        session.run.to_string(),
                        session.generation.get() as i64,
                        txn.now()
                    ],
                )?;
                if inserted != 1 {
                    return Err(RunError::refused("this attachment id is already in use"));
                }
                Ok((id, fence))
            }
        }
    })?;
    crate::fault_point!(RunAttachAfter)?;
    Ok(Attachment {
        id,
        run,
        fence,
        session,
        client,
        cursors: BTreeMap::new(),
        released: false,
    })
}

fn release_attachment(
    cx: &EffectContext,
    root: &StateRoot,
    attachment: &Attachment,
    cause: Release,
) -> Result<(), RunError> {
    mutations_only(cx, "a workspace attachment release")?;
    let cx = &cx.child(std::time::Duration::from_secs(1));
    let mut store = Store::open_within(cx, root, BusyWait::Context)?;
    crate::fault_point!(RunAttachReleaseBefore)?;
    store.transaction_within(cx, BusyWait::Context, |txn| {
        let released = txn.conn().execute(
            "UPDATE run_attachments SET state='released',release=?2,released_at=?3 \
             WHERE attachment_id=?1 AND state='attached'",
            params![attachment.id.to_string(), release_name(cause), txn.now()],
        )?;
        if released == 1 && attachment.client == Client::Owner && cause == Release::Eof {
            stop_owner_run(txn, attachment)?;
        }
        Ok::<_, RunError>(())
    })?;
    crate::fault_point!(RunAttachReleaseAfter)?;
    Ok(())
}

/// An owning client's EOF without a detach stops its run (§8.11). The owner
/// loop already drains these requests before it interprets EOF on its pipe, so
/// the stop is durable even though the pipe is gone. A detach records none.
fn stop_owner_run(txn: &Txn<'_>, attachment: &Attachment) -> Result<(), RunError> {
    let Some(record) = journal_run(txn.conn(), attachment.run)? else {
        return Ok(());
    };
    if record.phase == Phase::Closed
        || record.owner != attachment.fence.resource.name().to_string()
        || record.generation != attachment.fence.generation
    {
        // The run already closed or moved on; the release is the whole record.
        return Ok(());
    }
    // A pending stop/escalation already guarantees owner shutdown. Reuse it
    // when the terminal filled its control budget before disappearing.
    let mut pending = txn
        .conn()
        .prepare("SELECT action FROM requests WHERE run_id=?1 AND state='pending'")?;
    for action in pending.query_map([attachment.run.to_string()], |row| row.get::<_, String>(0))? {
        let request: Request = serde_json::from_str(&action?)?;
        if request.run == attachment.run
            && request.expected == attachment.fence
            && matches!(
                request.action,
                RequestAction::Stop | RequestAction::Escalate
            )
        {
            return Ok(());
        }
    }
    let id = owner_eof_request(attachment);
    txn.conn().execute(
        "INSERT INTO requests(request_id,run_id,expected_generation,action,state,created_at) \
         VALUES (?1,?2,?3,?4,'pending',?5)",
        params![
            id.to_string(),
            attachment.run.to_string(),
            attachment.fence.generation.get() as i64,
            serde_json::to_string(&Request {
                id,
                run: attachment.run,
                expected: attachment.fence.clone(),
                action: RequestAction::Stop,
            })?,
            txn.now()
        ],
    )?;
    Ok(())
}

/// One owning client's EOF names one stop for its attachment, so a repeated
/// release cannot queue a second one.
fn owner_eof_request(attachment: &Attachment) -> RunId {
    let mut material = b"effigy.workspace.owner-eof-stop.v1".to_vec();
    material.extend_from_slice(attachment.id.bytes());
    let digest = Digest::of(&material);
    let mut random = [0u8; 10];
    random.copy_from_slice(&digest.0[..10]);
    RunId::new(0, random).expect("a derived request id fits the identity range")
}

fn release_name(cause: Release) -> &'static str {
    match cause {
        Release::Eof => "eof",
        Release::Detach => "detach",
    }
}

/// Whether this exact attachment row is still attached to this run and session.
pub(super) fn attached_row(
    conn: &Connection,
    id: AttachmentId,
    attachment: &Attachment,
) -> Result<bool, RunError> {
    let admitted = admission_session(conn, attachment.run)?;
    if admitted != Some(attachment.session) {
        return Ok(false);
    }
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM run_attachments \
         WHERE attachment_id=?1 AND run_id=?2 AND session_id=?3 AND session_generation=?4 AND state='attached')",
        params![
            id.to_string(),
            attachment.run.to_string(),
            attachment.session.run.to_string(),
            attachment.session.generation.get() as i64
        ],
        |r| r.get(0),
    )?)
}

/// The id of a run's owning attachment. It is derived from the run, so a
/// reconnect finds the one owner row and a second terminal can never mint one.
fn owner_attachment(run: RunId) -> AttachmentId {
    let mut material = b"effigy.workspace.owner-attachment.v1".to_vec();
    material.extend_from_slice(run.bytes());
    AttachmentId::derived(&material)
}

/// Both records are writes to a session's journal: host authority and Mutate
/// only. No guest placement and no Observe-only context can write one.
fn mutations_only(cx: &EffectContext, what: &str) -> Result<(), RunError> {
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::refused(format!("{what} needs Mutate")))?;
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| {
            RunError::refused(format!(
                "{what} needs host authority, and this placement is a guest"
            ))
        })?;
    Ok(())
}

/// The key that holds this run identity, if any. A run identity has at most
/// one such row (the unique index and `insert_reservation` both hold that
/// line), so this read can never answer with another session's key by accident.
fn admission_holder(conn: &Connection, run: RunId) -> Result<Option<AdmissionKey>, RunError> {
    conn.query_row(
        "SELECT session_id,session_generation,request_id FROM workspace_admissions WHERE run_id=?1",
        [run.to_string()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(session, generation, request)| {
        Ok::<_, RunError>(AdmissionKey {
            session: Session {
                run: session
                    .parse()
                    .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
                generation: Generation::new(generation as u64),
            },
            request: HexId::parse(&request)
                .map_err(|_| RunError::broken("a stored request id is not readable"))?,
        })
    })
    .transpose()
}

/// The session that actually admitted this run. A reservation that never
/// reached its admission holds the run identity but admits nothing, so it is
/// never reported here: an ordinary run that happened to reuse the id cannot be
/// attached as, or closed for, that key.
fn admission_session(conn: &Connection, run: RunId) -> Result<Option<Session>, RunError> {
    conn.query_row(
        "SELECT session_id,session_generation,session_owner FROM workspace_admissions \
             WHERE run_id=?1 AND state='admitted'",
        [run.to_string()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(session, generation, owner)| {
        let session = Session {
            run: session
                .parse()
                .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
            generation: Generation::new(generation as u64),
        };
        current_session(
            conn,
            session,
            Some(
                owner
                    .as_deref()
                    .ok_or_else(|| RunError::refused("the recorded session owner is unknown"))?,
            ),
        )?;
        Ok::<_, RunError>(session)
    })
    .transpose()
}

/// A session is an actual current run, not caller-chosen identity bytes.
fn current_session(
    conn: &Connection,
    session: Session,
    expected_owner: Option<&str>,
) -> Result<String, RunError> {
    let record = journal_run(conn, session.run)?
        .ok_or_else(|| RunError::refused("the workspace session is not admitted"))?;
    if record.phase == Phase::Closed
        || record.generation != session.generation
        || expected_owner.is_some_and(|owner| owner != record.owner)
    {
        return Err(RunError::refused("the workspace session identity is stale"));
    }
    let invocation: String = conn.query_row(
        "SELECT invocation FROM runs WHERE run_id=?1",
        [session.run.to_string()],
        |r| r.get(0),
    )?;
    let invocation = ResolvedInvocation::decode(invocation.as_bytes())
        .map_err(|error| RunError::broken(format!("the stored session invocation: {error}")))?;
    if invocation.spec().owner.name().to_string() != record.owner {
        return Err(RunError::refused("the workspace session owner changed"));
    }
    journal::check_fence(conn, &fence(conn, &record.owner, record.generation)?)?;
    Ok(record.owner)
}

pub(super) fn require_live_owner(
    cx: &ObserveContext,
    root: &StateRoot,
    conn: &Connection,
    run: RunId,
) -> Result<(), RunError> {
    let record = journal_run(conn, run)?
        .ok_or_else(|| RunError::refused("this run is no longer in its journal"))?;
    if record.phase == Phase::Closed {
        return Err(RunError::refused("this run is closed"));
    }
    live_owner(cx, root, conn, run, &record)
}

/// A historical run row alone cannot establish a serviceable supervisor.
fn live_owner(
    cx: &ObserveContext,
    root: &StateRoot,
    conn: &Connection,
    run: RunId,
    record: &JournalRun,
) -> Result<(), RunError> {
    let fence = fence(conn, &record.owner, record.generation)?;
    journal::check_fence(conn, &fence)?;
    let identity: Option<String> = conn.query_row(
        "SELECT owner_identity FROM runs WHERE run_id=?1",
        [run.to_string()],
        |r| r.get(0),
    )?;
    let identity: crate::ident::ProcessIdentity = serde_json::from_str(
        &identity
            .ok_or_else(|| RunError::unavailable("the run has no recoverable owner identity"))?,
    )?;
    let holder_matches = match Gate::probe(cx, root, &fence.resource) {
        Observed::Verified(crate::gate::GateState::Held(holder), _) => {
            holder.verified().is_some_and(|holder| {
                holder.pid == identity.pid
                    && holder.run.as_deref() == Some(run.to_string().as_str())
                    && holder.start.as_deref() == Some(identity.start.text().as_str())
                    && holder.boot.as_deref() == Some(identity.boot.as_str())
            })
        }
        _ => false,
    };
    if crate::ident::process_identity(identity.pid).verified() != Some(&identity)
        || !matches!(
            crate::ident::process_state(identity.pid),
            Observed::Verified(
                crate::ident::ProcessState::Active | crate::ident::ProcessState::Stopped,
                _
            )
        )
        || !holder_matches
    {
        return Err(RunError::unavailable(
            "the run owner is unavailable; custody must be recovered",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Reserved,
    Admitted,
    Closed,
}
impl State {
    fn parse(text: &str) -> Result<Self, RunError> {
        match text {
            "reserved" => Ok(Self::Reserved),
            "admitted" => Ok(Self::Admitted),
            "closed" => Ok(Self::Closed),
            other => Err(RunError::broken(format!(
                "a workspace admission state this store wrote is not readable (`{other}`)"
            ))),
        }
    }
}

#[derive(Debug)]
struct Row {
    session_owner: Option<String>,
    digest: Digest,
    run: RunId,
    state: State,
    outcome: Option<RecordedOutcome>,
}

fn row(conn: &Connection, key: &AdmissionKey) -> Result<Option<Row>, RunError> {
    conn.query_row(
        "SELECT digest,run_id,state,outcome,session_owner FROM workspace_admissions \
         WHERE session_id=?1 AND session_generation=?2 AND request_id=?3",
        params![
            key.session.run.to_string(),
            key.session.generation.get() as i64,
            key.request.to_string()
        ],
        |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    )
    .optional()?
    .map(|(digest, run, state, outcome, session_owner)| {
        let bytes: [u8; 32] = digest
            .as_slice()
            .try_into()
            .map_err(|_| RunError::broken("a stored request digest is not 32 bytes"))?;
        Ok(Row {
            session_owner,
            digest: Digest(bytes),
            run: run
                .parse()
                .map_err(|error: crate::id::IdentityError| RunError::Store(error.into()))?,
            state: State::parse(&state)?,
            outcome: outcome
                .map(|text| serde_json::from_str(&text))
                .transpose()?,
        })
    })
    .transpose()
}

#[derive(Debug)]
struct JournalRun {
    owner: String,
    generation: Generation,
    phase: Phase,
    workload: Option<WorkloadOutcome>,
    cleanup: Option<String>,
}

fn journal_run(conn: &Connection, id: RunId) -> Result<Option<JournalRun>, RunError> {
    conn.query_row(
        "SELECT owner,generation,phase,workload_outcome,cleanup_outcome FROM runs WHERE run_id=?1",
        [id.to_string()],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        },
    )
    .optional()?
    .map(|(owner, generation, phase, workload, cleanup)| {
        Ok(JournalRun {
            owner,
            generation: Generation::new(generation as u64),
            phase: Phase::parse(&phase)?,
            workload: workload
                .map(|text| serde_json::from_str(&text))
                .transpose()?,
            cleanup,
        })
    })
    .transpose()
}

/// The run's owner fence: the store's canonical name is the authority for the
/// resource identity, never a value the caller supplied.
fn fence(conn: &Connection, owner: &str, generation: Generation) -> Result<Fence, RunError> {
    let canonical: Vec<u8> = conn.query_row(
        "SELECT canonical FROM resources WHERE name=?1",
        [owner],
        |r| r.get(0),
    )?;
    let resource =
        ResourceId::decode_canonical(&canonical).map_err(|error: crate::id::IdentityError| {
            RunError::broken(format!(
                "a resource name this store wrote is not readable ({error})"
            ))
        })?;
    Ok(Fence {
        resource,
        generation,
    })
}

impl Run<'_> {
    pub(crate) fn workspace_attachment_attached(
        &self,
        attachment: &Attachment,
    ) -> Result<bool, RunError> {
        if attachment.run != self.id || attachment.fence != self.owner {
            return Err(RunError::refused("this attachment belongs to another run"));
        }
        attached_row(self.store.conn(), attachment.id, attachment)
    }

    pub(crate) fn release_workspace_attachment(
        &self,
        attachment: &mut Attachment,
    ) -> Result<(), RunError> {
        if !self.workspace_attachment_attached(attachment)? {
            return Ok(());
        }
        let cleanup = self
            .cx
            .operation()
            .cleanup(std::time::Duration::from_secs(1))
            .map_err(|_| RunError::broken("attachment cleanup budget overflow"))?;
        let cx = cleanup
            .operation()
            .effects()
            .map_err(|_| RunError::refused("attachment release needs effects"))?;
        attachment.release(&cx, self.root, Release::Eof)
    }

    pub(crate) fn workspace_owner_detached(&self) -> Result<bool, RunError> {
        Ok(self.store.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM run_attachments WHERE run_id=?1 AND lifetime='owner' AND state='released' AND release='detach')",
            [self.id.to_string()], |row| row.get(0),
        )?)
    }
}

#[cfg(test)]
mod tests;
