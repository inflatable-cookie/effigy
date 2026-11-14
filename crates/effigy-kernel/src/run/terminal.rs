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
// A route that writes this row must have a finite per-invocation footprint.
// Identity fields are **validated, never cut**: `run_id` is the primary key, so
// cutting two runs to one key would coalesce their evidence, and a cut
// `checkout` or `boot` would name a different runtime. An oversized identity is
// refused and no row is written. The `task` label is descriptive, so it is cut
// with [`TRUNCATION_MARKER`] and the loss stays visible in the row. The maxima
// are wide against what the writers produce: a `RunId` renders in 26
// characters, `CheckoutId::text` in 34, a boot identity is a 36-character UUID
// and `outcome`/`cleanup` are words from a closed set.
/// `run_id`: the primary key this journal records.
pub const RUN_ID_MAX: usize = 64;
/// `checkout`: a `CheckoutId` in text form.
pub const CHECKOUT_MAX: usize = 128;
/// `boot`: a boot identity in text form.
pub const BOOT_MAX: usize = 64;
/// `task`: the admitted task's label. Cut, not refused.
pub const TASK_MAX: usize = 512;
/// `outcome` and `cleanup`: two of a closed set of words, never cut.
pub const WORD_MAX: usize = 32;
/// Ends a label this writer cut. A reader therefore sees the loss in the row
/// itself; nothing presents a cut value as complete.
pub const TRUNCATION_MARKER: &str = "[truncated]";
/// Expired rows one terminal write prunes. The prune is one indexed delete
/// limited to this many rows, so it touches a fixed number of rows; the
/// retention sweep still removes every expired row, so the seven-day retention
/// is unchanged (see `crate::retention`).
pub const PRUNE_MAX: u32 = 32;
/// Fixed bytes the stored row adds beside its text: the two INTEGER columns
/// (8 bytes each), their varint headers, the record header and the cell header.
pub const ROW_OVERHEAD_BYTES: usize = 64;
/// The largest row this module writes. Enforced at write time by
/// [`TerminalRecord::checked`].
pub const ROW_MAX_BYTES: usize =
    RUN_ID_MAX + CHECKOUT_MAX + BOOT_MAX + TASK_MAX + 2 * WORD_MAX + ROW_OVERHEAD_BYTES;
/// The frames one **strict** terminal write appends, as a budget the run
/// verifies rather than a theorem (effigy#981).
///
/// A closed-form worst case for one SQLite statement is not available from the
/// bundled source, and the earlier attempt at one was wrong in three places, so
/// this module no longer claims it:
///
/// - `balance_nonroot` (`sqlite3.c`, `apOld[NB]` and `apNew[NB+2]` with
///   `NB` = 3) balances up to three sibling pages into up to five new ones and
///   rewrites their parent, so one level dirties up to nine pages, not two.
/// - The same balance writes a pointer-map entry for every cell it moves into a
///   new page (and for that page), and those entries can live in many distinct
///   pointer-map pages, so the pointer-map term is a number of scattered pages,
///   not one or two.
/// - The tree depth therefore cannot be argued down to a small constant from
///   the store's page cap alone.
///
/// What this module does instead is state a **budget** ([`STRICT_WRITE_MAX_PAGES`])
/// and make every strict run check it: `record` measures the frames its own
/// transaction appended and refuses to report success if they exceed the
/// budget. A store this build wrote touches one leaf in each of the three trees
/// (the table, the `run_id` unique index and the `closed_at` index) plus, at
/// worst, one split level in each, page 1 and the freelist: 3 x 6 + 3 = 21
/// frames. The measured contribution in every tested state is 3 frames
/// (`run_terminal_strict_write_appends_at_most_its_stated_frames_and_defers_the_prune`).
/// The premise-free ceiling is the store's own page count — SQLite writes each
/// modified page to the log at most once per transaction, and
/// `store::MAX_DB_BYTES` caps the store at [`crate::store::MAX_PAGES`] pages —
/// which is [`STRICT_WAL_CEILING_BYTES`].
pub const STRICT_WRITE_MAX_PAGES: u32 = 21;
/// The bytes [`STRICT_WRITE_MAX_PAGES`] occupies: SQLite writes a 32-byte log
/// header (`store::WAL_HEADER_BYTES`), then one 24-byte frame header before each
/// page (`store::WAL_FRAME_HEADER_BYTES`, `WAL_FRAME_HDRSIZE` in `wal.c`;
/// <https://sqlite.org/fileformat2.html#walformat>).
pub const STRICT_WAL_MAX_BYTES: u64 = crate::store::WAL_HEADER_BYTES
    + STRICT_WRITE_MAX_PAGES as u64
        * (crate::store::PAGE_SIZE + crate::store::WAL_FRAME_HEADER_BYTES);
/// The premise-free ceiling for one transaction's log: one frame per page the
/// store holds, at the store's own page cap.
pub const STRICT_WAL_CEILING_BYTES: u64 = crate::store::WAL_HEADER_BYTES
    + crate::store::MAX_PAGES * (crate::store::PAGE_SIZE + crate::store::WAL_FRAME_HEADER_BYTES);
/// The pages one INSERT of a bounded row dirties in the common case
/// (effigy#977): one leaf in each of the three trees, page 1 and the freelist.
/// The tests measure this budget rather than deriving a worst case; the row
/// itself never chains to overflow pages, because [`ROW_MAX_BYTES`] is smaller
/// than one page (asserted at compile time).
pub const INSERT_MAX_PAGES: u32 = 16;
/// The pages the bounded prune dirties (effigy#977): one leaf per pruned row in
/// each of the three trees, plus the balance path and the freed pages'
/// bookkeeping. Measured by the tests, never claimed as a worst case.
pub const PRUNE_MAX_PAGES: u32 = 2 * PRUNE_MAX + 2 * 8;
/// The frames one terminal write appends to the write-ahead log — the frames of
/// its own transaction, not the log file's size (effigy#977).
///
/// Its two inputs are enforced: the insert is refused unless the row fits
/// [`ROW_MAX_BYTES`] and one page ([`TerminalRecord::checked`]), and the prune
/// is one SQL statement with `LIMIT PRUNE_MAX`. The page counts are the budget
/// the tests measure against the statements this module issues; they are not a
/// modelled worst case (see [`STRICT_WRITE_MAX_PAGES`] for why).
pub const WRITE_MAX_PAGES: u32 = INSERT_MAX_PAGES + PRUNE_MAX_PAGES;
/// [`WRITE_MAX_PAGES`] in bytes at the page size Effigy writes.
pub const WRITE_MAX_BYTES: u64 = WRITE_MAX_PAGES as u64 * crate::store::PAGE_SIZE;
/// A row this module writes fits in one page, so it never takes an overflow
/// chain. Enforced by the build, not only by a test.
const _: () = assert!(ROW_MAX_BYTES <= crate::store::PAGE_SIZE as usize);

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
/// The row is bounded and its identities are validated here, at the one place
/// that writes it (effigy#977): a caller cannot make the row unbounded, and no
/// two runs can coalesce on a cut key.
///
/// Under strict persistence it also **verifies** the write's own log
/// contribution (effigy#981): [`STRICT_WRITE_MAX_PAGES`] is a budget, and a run
/// whose transaction appended more than that is reported instead of passing as
/// if it had stayed inside it. Nothing checkpoints here, so the frames measured
/// at the end are exactly the ones this write appended.
pub fn record(
    cx: &EffectContext,
    root: &StateRoot,
    record: &TerminalRecord,
) -> Result<(), StoreError> {
    let record = record.checked()?;
    let mut store = Store::open(cx, root)?;
    let strict = store.strict();
    let log_before = if strict {
        Some(store.wal_bytes()?)
    } else {
        None
    };
    store.txn(cx, |txn| write_row(txn, &record))?;
    if let Some(before) = log_before {
        let appended = store.wal_bytes()?.saturating_sub(before);
        let page = store.sidecar_bounds()?.page_size;
        let frames = appended.saturating_sub(crate::store::WAL_HEADER_BYTES)
            / page
                .saturating_add(crate::store::WAL_FRAME_HEADER_BYTES)
                .max(1);
        if frames > u64::from(STRICT_WRITE_MAX_PAGES) {
            return Err(StoreError::Refused(format!(
                "strict persistence appended {frames} write-ahead-log frames to this run's \
                 {STRICT_WRITE_MAX_PAGES}-frame budget ({} bytes); the row is recorded, and this \
                 run must not be counted as staying inside that budget",
                STRICT_WAL_MAX_BYTES
            )));
        }
    }
    Ok(())
}

/// The two statements one terminal write issues, in its own transaction: the
/// bounded row insert, then one indexed delete of at most `PRUNE_MAX` expired
/// rows. Split out so the page and frame arithmetic can be measured against
/// exactly the statements this module writes.
pub(crate) fn write_row(
    txn: &mut crate::store::Txn<'_>,
    record: &TerminalRecord,
) -> Result<(), StoreError> {
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
    // Strict persistence defers this prune (effigy#981): the row this write
    // inserts is then the invocation's whole database contribution, and one
    // fewer statement is one fewer log frame. A later non-strict write and the
    // retention sweep still remove every expired row, so the seven-day
    // retention is unchanged.
    if txn.strict() {
        return Ok(());
    }
    // Bounded retention: the record is evidence for a recent run. One indexed
    // delete of at most `PRUNE_MAX` expired rows per write keeps this write's
    // rows fixed; the retention sweep still removes every expired row.
    txn.conn().execute(
        "DELETE FROM foreground_terminals WHERE run_id IN (\
             SELECT run_id FROM foreground_terminals WHERE closed_at < ?1 \
             ORDER BY closed_at LIMIT ?2)",
        params![wall_ms() - KEEP_MS, PRUNE_MAX as i64],
    )?;
    Ok(())
}

impl TerminalRecord {
    /// The row this writer stores, or the reason it may not be written.
    ///
    /// Every identity field and every closed-set word must already fit its
    /// maximum: they name a run, a checkout, a boot or a state, so cutting one
    /// would make the evidence name something else. Only the descriptive `task`
    /// label is cut, and then the encoded row is refused if it would exceed
    /// [`ROW_MAX_BYTES`] — a check that keeps the bound enforced if a field is
    /// ever added.
    pub fn checked(&self) -> Result<Self, StoreError> {
        for (field, value, max) in [
            ("run_id", &self.run_id, RUN_ID_MAX),
            ("checkout", &self.checkout, CHECKOUT_MAX),
            ("boot", &self.boot, BOOT_MAX),
            ("outcome", &self.outcome, WORD_MAX),
            ("cleanup", &self.cleanup, WORD_MAX),
        ] {
            if value.len() > max {
                return Err(StoreError::Broken(format!(
                    "the terminal record's {field} is {} bytes, over its {max}-byte maximum; \
                     it names the run, so it is refused rather than cut and no record is written",
                    value.len()
                )));
            }
        }
        let record = self.bounded();
        let encoded = record.encoded_bytes();
        if encoded > ROW_MAX_BYTES {
            return Err(StoreError::Broken(format!(
                "the terminal record encodes {encoded} bytes, over the {ROW_MAX_BYTES}-byte \
                 row maximum"
            )));
        }
        Ok(record)
    }

    /// The bytes this row stores: every text value plus the fixed overhead of
    /// its two INTEGER columns and the record and cell headers.
    pub fn encoded_bytes(&self) -> usize {
        self.run_id.len()
            + self.checkout.len()
            + self.boot.len()
            + self.task.len()
            + self.outcome.len()
            + self.cleanup.len()
            + ROW_OVERHEAD_BYTES
    }

    /// The row this writer stores: the descriptive label cut to its maximum,
    /// ending with [`TRUNCATION_MARKER`] when it was cut. Identities and the
    /// closed-set words are left exactly as they are; [`Self::checked`] is what
    /// refuses those. An in-bounds record is returned unchanged.
    pub fn bounded(&self) -> Self {
        Self {
            task: bounded(&self.task, TASK_MAX),
            ..self.clone()
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
