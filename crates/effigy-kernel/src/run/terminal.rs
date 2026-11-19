//! The one terminal record per foreground run.
//!
//! A run that settles writes one row to the owning checkout journal, so `run
//! status` and `runs` can report its outcome after the process has gone.
//! Losing every custodian leaves nothing to read otherwise (052 §3.1).
//!
//! The row is **evidence**, never authority. It carries no PID, PGID or start
//! identity, so nothing here can be signalled, reclaimed or recovered from.
//! Effigy claims no closure from it beyond the run's own settled `cleanup`: a
//! record with `cleanup: "complete"` is the one custody closure this row
//! asserts. A run whose custodians all died before settle writes nothing.

use super::*;
use crate::ctx::EffectContext;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::store::{Store, StoreError, StoreReader};
use rusqlite::params;

/// How long a foreground terminal record is kept. The record is an
/// operator's evidence for a recently finished run, not history. The retention
/// sweep removes expired rows.
pub const KEEP_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// The evidence name `run status` reports for this record.
pub const EVIDENCE: &str = "foreground-terminal-record.v1";
const TABLE: &str = "foreground_terminals";
const COLUMNS: &str = "run_id, checkout, boot, task, outcome, exit_code, signal, \
    cleanup, closed_at";

/// One run's settled result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalRecord {
    pub run_id: String,
    /// The CheckoutId the journal belongs to.
    pub checkout: String,
    /// The boot the run's custodians lived in.
    pub boot: String,
    pub task: String,
    /// `not-started`, `succeeded`, `failed`, `cancelled`, `timed-out`,
    /// `interrupted`, `released` or `unknown`, as reported by `run status`.
    pub outcome: String,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    /// `complete`, `incomplete` or `unknown`.
    pub cleanup: String,
    pub closed_at_ms: i64,
}

/// Writes one run's terminal record. Idempotent by run id: an existing row is
/// kept, so a repeated settle never rewrites evidence. The one exception is an
/// `unknown`/`incomplete` parked record. Once custody closes, the owner or
/// sentinel may upgrade it to `interrupted`/`complete`. A sentinel that honors
/// an explicit release may upgrade it to `released`/`complete` (effigy#1034).
pub fn record(
    cx: &EffectContext,
    root: &StateRoot,
    record: &TerminalRecord,
) -> Result<(), StoreError> {
    let mut store = Store::open(cx, root)?;
    store.txn(cx, |txn| {
        txn.conn().execute(
            "INSERT INTO foreground_terminals \
             (run_id, checkout, boot, task, outcome, exit_code, signal, cleanup, closed_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(run_id) DO UPDATE SET \
               outcome=excluded.outcome, cleanup=excluded.cleanup, \
               closed_at=excluded.closed_at \
             WHERE foreground_terminals.outcome = 'unknown' \
               AND foreground_terminals.cleanup = 'incomplete' \
               AND excluded.cleanup = 'complete' \
               AND excluded.outcome IN ('released', 'interrupted')",
            params![
                record.run_id,
                record.checkout,
                record.boot,
                record.task,
                record.outcome,
                record.exit_code,
                record.signal,
                record.cleanup,
                record.closed_at_ms,
            ],
        )?;
        Ok(())
    })
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
                closed_at_ms: row.get(8)?,
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
