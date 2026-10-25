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
pub fn record(
    cx: &EffectContext,
    root: &StateRoot,
    record: &TerminalRecord,
) -> Result<(), StoreError> {
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
        // indexed delete per write keeps the table small without a scan.
        txn.conn().execute(
            "DELETE FROM foreground_terminals WHERE closed_at < ?1",
            params![wall_ms() - KEEP_MS],
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
