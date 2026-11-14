//! The narrow foreground terminal record (Q-010 exception, effigy#325).
//!
//! Q-010 keeps a short foreground run in memory, so losing every custodian
//! leaves nothing to read (052 §3.1). An orchestrator that opts in to the v2
//! admission protocol asks its hook to witness each workload group, and
//! Effigy writes exactly one row to the owning checkout journal at settle.
//!
//! The row is **evidence**, never authority. It carries no PID, PGID or
//! start identity, so nothing here can be signalled, reclaimed or recovered
//! from. Effigy claims no closure from it beyond the run's own settled
//! `cleanup`: a record with `cleanup: "complete"` is the one custody closure
//! this exception asserts (052 R5/I-R7). A run whose custodians all died
//! before settle writes nothing.

use super::*;
use crate::ctx::EffectContext;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::store::{wall_ms, Store, StoreError, StoreReader};
use rusqlite::params;

/// How long a foreground terminal record is kept. The record is an
/// operator's evidence for a recently finished run, not history.
pub const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// The evidence name `run status` reports for this record.
pub const EVIDENCE: &str = "foreground-terminal-record.v1";
const TABLE: &str = "foreground_terminals";
const COLUMNS: &str = "run_id, checkout, boot, task, outcome, exit_code, signal, \
    cleanup, workload_groups, closed_at";

// Hard maxima for the row's variable-length fields, in bytes (effigy#977).
//
// A route that writes this row must have a finite per-invocation footprint,
// so every field is cut at write time and a cut field ends with
// [`TRUNCATION_MARKER`]. The maxima are generous against what the writers
// produce: a `RunId` renders in 26 characters, `CheckoutId::text` in 32-34,
// a boot identity in 36 and a task label is a selector name.
/// `run_id`: the widest identity this journal records.
pub const RUN_ID_MAX: usize = 64;
/// `checkout`: a `CheckoutId` in text form.
pub const CHECKOUT_MAX: usize = 128;
/// `boot`: a boot identity in text form.
pub const BOOT_MAX: usize = 128;
/// `task`: the admitted task's label.
pub const TASK_MAX: usize = 512;
/// `outcome` and `cleanup`: two of a closed set of words.
pub const WORD_MAX: usize = 32;
/// Ends a field this writer cut. A reader therefore sees the loss in the row
/// itself; nothing presents a cut value as complete.
pub const TRUNCATION_MARKER: &str = "[truncated]";
/// Expired rows one terminal write prunes. The write's page touches stay
/// fixed; the retention sweep still removes every expired row, so the
/// seven-day retention is unchanged (see `crate::retention`).
pub const PRUNE_MAX: u32 = 32;
/// The pages one terminal write can append to the write-ahead log, and so the
/// bytes it can add to that side file (effigy#977).
///
/// A store is capped at `store::MAX_DB_BYTES` = 1 GiB = 262144 pages of 4096
/// bytes, so a b-tree over it is at most four levels deep and one insert
/// touches at most eight pages, including a split path: two trees (the table
/// and its `closed_at` index) give at most sixteen. The bounded prune touches
/// one leaf and one index leaf per expired row (2 × 32 = 64) and at most
/// sixteen interior pages: 96 in all. The bound below is stated above that
/// arithmetic so it holds with slack rather than exactly.
pub const WRITE_MAX_PAGES: u32 = 128;
/// [`WRITE_MAX_PAGES`] in bytes at the page size Effigy writes.
pub const WRITE_MAX_BYTES: u64 = WRITE_MAX_PAGES as u64 * 4096;
/// The largest row this module writes: the maxima above plus a fixed 64 bytes
/// for the row's two INTEGER columns, their varint headers and the record and
/// cell headers SQLite adds around the values.
pub const ROW_MAX_BYTES: usize =
    RUN_ID_MAX + CHECKOUT_MAX + BOOT_MAX + TASK_MAX + 2 * WORD_MAX + 64;

/// One v2 hook-admitted foreground run's settled result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRecord {
    pub run_id: String,
    /// The CheckoutId the journal belongs to.
    pub checkout: String,
    /// The boot the run's custodians lived in.
    pub boot: String,
    pub task: String,
    /// `succeeded`, `failed`, `cancelled`, `timed-out`, `interrupted` or
    /// `unknown`, the same words `run status` uses.
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    /// `complete`, `incomplete` or `unknown`.
    pub cleanup: String,
    pub workload_groups: u64,
    pub closed_at_ms: i64,
}

/// Writes one run's terminal record. Idempotent by run id; an existing row is
/// kept, so a repeated settle never rewrites evidence.
///
/// Every variable-length field is bounded here, at the one place that writes
/// the row, so a caller cannot make a row unbounded (effigy#977).
pub fn record(
    cx: &EffectContext,
    root: &StateRoot,
    record: &TerminalRecord,
) -> Result<(), StoreError> {
    let record = record.bounded();
    let mut store = Store::open(cx, root)?;
    store.txn(cx, |txn| {
        txn.conn().execute(
            "INSERT INTO foreground_terminals \
             (run_id, checkout, boot, task, outcome, exit_code, signal, cleanup, \
              workload_groups, closed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
             ON CONFLICT(run_id) DO NOTHING",
            params![
                record.run_id,
                record.checkout,
                record.boot,
                record.task,
                record.outcome,
                record.exit_code,
                record.signal,
                record.cleanup,
                record.workload_groups as i64,
                record.closed_at_ms,
            ],
        )?;
        // Bounded retention: the record is evidence for a recent run. One
        // indexed delete of at most `PRUNE_MAX` expired rows per write keeps
        // this write's page touches fixed; the retention sweep still removes
        // every expired row.
        txn.conn().execute(
            "DELETE FROM foreground_terminals WHERE run_id IN (\
                 SELECT run_id FROM foreground_terminals WHERE closed_at < ?1 \
                 ORDER BY closed_at LIMIT ?2)",
            params![wall_ms() - KEEP_MS, PRUNE_MAX as i64],
        )?;
        Ok(())
    })
}

impl TerminalRecord {
    /// The row this writer stores: every variable-length field cut to its hard
    /// maximum, ending with [`TRUNCATION_MARKER`] when it was cut. Fixed-size
    /// fields (the two integers) are unchanged, and an unchanged record is
    /// returned unchanged.
    pub fn bounded(&self) -> Self {
        Self {
            run_id: bounded(&self.run_id, RUN_ID_MAX),
            checkout: bounded(&self.checkout, CHECKOUT_MAX),
            boot: bounded(&self.boot, BOOT_MAX),
            task: bounded(&self.task, TASK_MAX),
            outcome: bounded(&self.outcome, WORD_MAX),
            cleanup: bounded(&self.cleanup, WORD_MAX),
            exit_code: self.exit_code,
            signal: self.signal,
            workload_groups: self.workload_groups,
            closed_at_ms: self.closed_at_ms,
        }
    }
}

/// `value` cut to at most `max` bytes. A cut value still ends on a character
/// boundary and ends with the marker, so `max` is a byte ceiling rather than a
/// hint: `bounded(value, max).len() <= max` always holds.
fn bounded(value: &str, max: usize) -> String {
    debug_assert!(max >= TRUNCATION_MARKER.len());
    if value.len() <= max {
        return value.to_owned();
    }
    let keep = max - TRUNCATION_MARKER.len();
    let mut cut = 0;
    for (offset, character) in value.char_indices() {
        let end = offset + character.len_utf8();
        if end > keep {
            break;
        }
        cut = end;
    }
    let mut bounded = String::with_capacity(cut + TRUNCATION_MARKER.len());
    bounded.push_str(&value[..cut]);
    bounded.push_str(TRUNCATION_MARKER);
    bounded
}

/// One run's record, `Absent` when the journal has none (never recorded, or
/// expired by retention).
pub fn read(reader: &StoreReader, run: RunId) -> Observed<Option<TerminalRecord>> {
    match query(
        reader,
        &format!("SELECT {COLUMNS} FROM {TABLE} WHERE run_id=?1"),
        params![run.to_string()],
    ) {
        Observed::Verified(rows, evidence) => match rows.into_iter().next() {
            Some(record) => Observed::Verified(Some(record), evidence),
            None => Observed::Absent(AbsenceProof::NoRow {
                table: TABLE.into(),
            }),
        },
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// The newest records in this journal, newest closed first.
pub fn recent(reader: &StoreReader, limit: usize) -> Observed<Vec<TerminalRecord>> {
    query(
        reader,
        &format!("SELECT {COLUMNS} FROM {TABLE} ORDER BY closed_at DESC, run_id DESC LIMIT ?1"),
        params![limit as i64],
    )
}

/// A store written by an older build has no such table: that is `Absent`
/// evidence, not an unreadable journal.
fn query(
    reader: &StoreReader,
    sql: &str,
    params: impl rusqlite::Params,
) -> Observed<Vec<TerminalRecord>> {
    match reader.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        params![TABLE],
        |row| row.get::<_, bool>(0),
    ) {
        Ok(true) => {}
        Ok(false) => {
            return Observed::Absent(AbsenceProof::NoRow {
                table: TABLE.into(),
            });
        }
        Err(error) => {
            return Observed::Unknown(UnknownReason::Inconsistent(error.to_string()));
        }
    }
    let rows = (|| -> Result<Vec<TerminalRecord>, RunError> {
        let mut statement = reader.conn().prepare(sql)?;
        let mapped = statement.query_map(params, |row| {
            Ok(TerminalRecord {
                run_id: row.get(0)?,
                checkout: row.get(1)?,
                boot: row.get(2)?,
                task: row.get(3)?,
                outcome: row.get(4)?,
                exit_code: row.get(5)?,
                signal: row.get(6)?,
                cleanup: row.get(7)?,
                workload_groups: row.get::<_, i64>(8)?.max(0) as u64,
                closed_at_ms: row.get(9)?,
            })
        })?;
        let mut records = Vec::new();
        for record in mapped {
            records.push(record?);
        }
        Ok(records)
    })();
    match rows {
        Ok(records) => Observed::Verified(
            records,
            Evidence::new("foreground terminal record".into(), None)
                .expect("fixed evidence source"),
        ),
        Err(error) => Observed::Unknown(UnknownReason::Inconsistent(error.to_string())),
    }
}
