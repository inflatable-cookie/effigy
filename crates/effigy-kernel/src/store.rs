//! Runtime records (§5.5, §7.2): checkout `kernel.db` or machine `machine.db`
//! under a verified state root, with typed multi-record transactions.
//!
//! SQLite opens `kernel.db` by path. The kernel first creates or verifies the
//! file descriptor-relative (regular, operator-owned, 0600, one link, bounded)
//! inside a verified 0700 root, and SQLite then opens it with
//! `SQLITE_OPEN_NOFOLLOW`, so no symlinked component is followed (K13).
//!
//! Writes run in write-ahead logging (§5.5): one short `BEGIN IMMEDIATE`
//! transaction at a time, serialized by the kernel's own store writer gate so
//! a busy store always names its holder. A caller's workload deadline never
//! shortens the wait below the store's own window, and settle keeps waiting
//! for two minutes: a busy store delays a run, it never loses its outcome.

use crate::ctx::{EffectClass, EffectContext, Interrupt, ObserveContext};
use crate::fs::{FileName, FsError, IoError, Trust, FILE_MODE};
use crate::gate::{self, Gate, GateError, GateGuard, HolderDiag, OwnerState, RunPhaseFact, Wait};
use crate::human::Category;
use crate::id::{CheckoutId, Generation, IdentityError, ResourceId, RunId, Scope};
use crate::ident::BootInstant;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: i64 = 1;
pub const DB_NAME: &str = "kernel.db";
/// Size bound for `kernel.db`; retention keeps it far below this.
pub const MAX_DB_BYTES: u64 = 1 << 30;
const PAGE_SIZE: u64 = 4096;
/// The least one write waits for the store writer gate. Admission uses it
/// even when the caller's remaining operation budget is shorter (§5.5).
pub const STORE_BUSY_WAIT: Duration = Duration::from_secs(30);
/// Settle and outcome writes keep trying long after any workload deadline:
/// a busy store must never cost a run its recorded outcome (K4).
pub const SETTLE_BUSY_WAIT: Duration = Duration::from_secs(120);
/// A checkpoint truncates the write-ahead log to at most this size.
const WAL_LIMIT_BYTES: u64 = 16 << 20;
const BUSY_RETRY_MIN: Duration = Duration::from_millis(10);
const BUSY_RETRY_MAX: Duration = Duration::from_millis(250);
/// The writer gate every store transaction holds: the store resource itself.
const WRITER_GATE: &str = "kernel";
const BUILD: &str = concat!("effigy-kernel ", env!("CARGO_PKG_VERSION"));
/// A read waits at most this long for a SQLite lock: a plain read takes no
/// lock of its own, so a longer wait only hides a stuck store.
const READ_BUSY_WAIT: Duration = Duration::from_millis(250);

thread_local! { static IN_TXN: Cell<bool> = const { Cell::new(false) }; }
/// True while this thread is inside `Store::txn`. Gate acquisition and later
/// custody and probe primitives refuse here (K14).
pub fn in_transaction() -> bool {
    IN_TXN.with(Cell::get)
}
struct TxnFlag;
impl TxnFlag {
    fn enter() -> Result<Self, StoreError> {
        if IN_TXN.with(|flag| flag.replace(true)) {
            return Err(StoreError::InTransaction);
        }
        Ok(Self)
    }
}
impl Drop for TxnFlag {
    fn drop(&mut self) {
        IN_TXN.with(|flag| flag.set(false));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    HostAuthorityRequired,
    Untrusted {
        path: PathBuf,
        reason: String,
    },
    UnsupportedFilesystem {
        path: PathBuf,
        fs: String,
    },
    Io(IoError),
    Sqlite {
        code: i32,
        message: String,
    },
    /// The store's writer wait elapsed. The holder is a fresh gate
    /// diagnostic naming the process (and run) that still holds the store
    /// open for writing; the condition is environmental, not a task failure.
    Busy {
        waited: Duration,
        holder: Box<Observed<HolderDiag>>,
    },
    /// Mutate refuses; `Store::repair` is the explicit repair (moves it aside).
    Corrupt(String),
    UnknownSchema {
        found: i64,
    },
    Identity(IdentityError),
    /// Store waits need a finite deadline (K8, I-CX2).
    Unbounded,
    /// Retention, repair and scope removal run only in Mutate (K15).
    ClassRefused(EffectClass),
    InTransaction,
    Interrupted(Interrupt),
    Gate(Box<GateError>),
    /// `atomic_replace` needs a held store gate.
    /// A rule said no: the write guard the change needed, or checkout contents
    /// offered to the shared machine store.
    Refused(String),
    TooLarge {
        limit: u64,
    },
    NotCorrupt,
    /// A pointer commit must select a strictly newer generation.
    StaleGeneration {
        current: u64,
    },
    UnknownResource,
    /// Effigy's own record or an invariant is wrong, but nothing on disk is
    /// corrupt: an Effigy bug, and never a repair.
    Broken(String),
}
impl StoreError {
    pub fn category(&self) -> Category {
        match self {
            // A newer journal is version skew, not damage: the newer Effigy
            // reads it, and nothing here is corrupt.
            Self::UnknownSchema { found } if *found > SCHEMA_VERSION => Category::Unavailable,
            Self::Corrupt(_) | Self::UnknownSchema { .. } | Self::Broken(_) => Category::Broken,
            Self::InTransaction => Category::Broken,
            // A cancellation is not a rule refusing the request.
            Self::Interrupted(_) => Category::Unavailable,
            Self::HostAuthorityRequired
            | Self::Refused(_)
            | Self::ClassRefused(_)
            | Self::TooLarge { .. }
            | Self::NotCorrupt
            | Self::StaleGeneration { .. }
            | Self::UnknownResource => Category::Refused,
            Self::Untrusted { .. }
            | Self::UnsupportedFilesystem { .. }
            | Self::Io(_)
            | Self::Sqlite { .. }
            | Self::Busy { .. }
            | Self::Unbounded
            | Self::Identity(_)
            | Self::Gate(_) => Category::Unavailable,
        }
    }
}
/// The one next step verified corruption carries (§5.5): the store moves aside
/// with its write-ahead log, so no leftover log attaches to the next
/// `kernel.db`.
const CORRUPT_REPAIR: &str = "the explicit repair moves `kernel.db` and its `-journal`, `-wal` \
and `-shm` files aside, and then this command works";
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Verified corruption is the one case that says `Corrupt` and names the
        // repair; every other error renders through its category's one display.
        match self {
            Self::Corrupt(reason) => {
                return write!(
                    f,
                    "Effigy is broken: kernel.db is corrupt ({reason}); nothing was changed, and \
                     {CORRUPT_REPAIR}"
                )
            }
            Self::UnknownSchema { found } if *found > SCHEMA_VERSION => {
                return write!(
                    f,
                    "Effigy was updated: kernel.db was written by a newer Effigy at schema version \
                     {found}, which this binary cannot read; nothing was changed, and running the \
                     newer Effigy works"
                )
            }
            Self::UnknownSchema { found } => {
                return write!(
                    f,
                    "Effigy is broken: kernel.db is corrupt (it records schema version {found}, \
                     which this Effigy cannot read); nothing was changed, and {CORRUPT_REPAIR}"
                )
            }
            // A cancellation says what ended the work and asks nothing else of
            // the reader (F7).
            Self::Interrupted(interrupt @ Interrupt::Cancelled { .. }) => {
                return write!(f, "the state store was {interrupt}")
            }
            Self::Gate(error) => return error.fmt(f),
            _ => {}
        }
        let detail = match self {
            Self::Busy { waited, holder } => format!(
                "the state store kernel.db stayed busy for {:.1}s{} while another Effigy process \
                 wrote its run journal",
                waited.as_secs_f64(),
                holder_note(holder)
            ),
            Self::HostAuthorityRequired => {
                "the kernel store lives on the host, and a guest placement cannot open it"
                    .to_owned()
            }
            Self::Untrusted { path, reason } if reason == crate::fs::OPEN_MODE_REASON => format!(
                "{} is readable by other users, so run `chmod 700 {}`",
                path.display(),
                path.display()
            ),
            Self::Untrusted { path, reason } => {
                format!("{} is not trusted store state ({reason})", path.display())
            }
            Self::UnsupportedFilesystem { path, fs } => format!(
                "{} is on a network filesystem ({fs}), and the store needs a local disk",
                path.display()
            ),
            Self::Sqlite { code, message } => {
                format!("kernel.db refused the query ({message}, SQLite code {code})")
            }
            Self::Io(error) => error.to_string(),
            Self::Identity(error) => format!("kernel.db failed its identity check: {error}"),
            Self::Broken(check) => check.clone(),
            Self::Unbounded => {
                "a store wait needs a finite deadline, so Effigy refused an unbounded one"
                    .to_owned()
            }
            Self::ClassRefused(class) => format!(
                "a {} operation may not run this store effect; retention, repair and scope removal \
                 need a Mutate operation",
                crate::human::effect_class_label(*class)
            ),
            Self::InTransaction => {
                "a store transaction is already open on this thread, and nesting it is not allowed"
                    .to_owned()
            }
            Self::Interrupted(interrupt @ Interrupt::TimedOut { .. }) => interrupt.to_string(),
            Self::Refused(rule) => rule.clone(),
            Self::TooLarge { limit } => {
                format!("the store refused a value above its {limit}-byte limit")
            }
            Self::NotCorrupt => "kernel.db is intact, so there is nothing to repair".to_owned(),
            Self::StaleGeneration { current } => format!(
                "the store is already at generation {current}, and a pointer commit must select a \
                 strictly newer one"
            ),
            Self::UnknownResource => {
                "the store has no record of that resource, so the run that named it may have been \
                 removed"
                    .to_owned()
            }
            Self::Corrupt(_)
            | Self::UnknownSchema { .. }
            | Self::Gate(_)
            | Self::Interrupted(Interrupt::Cancelled { .. }) => unreachable!("rendered above"),
        };
        f.write_str(&self.category().render("the state store", &detail))
    }
}

impl std::error::Error for StoreError {}
impl From<FsError> for StoreError {
    fn from(error: FsError) -> Self {
        match error {
            FsError::HostAuthorityRequired => Self::HostAuthorityRequired,
            FsError::Untrusted { path, reason } => Self::Untrusted { path, reason },
            FsError::UnsupportedFilesystem { path, fs } => Self::UnsupportedFilesystem { path, fs },
            FsError::Io(error) => Self::Io(error),
        }
    }
}
impl From<GateError> for StoreError {
    fn from(error: GateError) -> Self {
        Self::Gate(Box::new(error))
    }
}
impl From<IdentityError> for StoreError {
    fn from(error: IdentityError) -> Self {
        Self::Identity(error)
    }
}
impl From<rusqlite::Error> for StoreError {
    fn from(error: rusqlite::Error) -> Self {
        let Some(code) = error.sqlite_error() else {
            return Self::Sqlite {
                code: -1,
                message: error.to_string(),
            };
        };
        let io = |errno| {
            Self::Io(IoError {
                op: "sqlite",
                path: DB_NAME.into(),
                errno: Some(errno),
                message: error.to_string(),
            })
        };
        match code.code {
            // Verified corruption: the only codes that may say `Corrupt`.
            ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => {
                Self::Corrupt(error.to_string())
            }
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => Self::Busy {
                waited: Duration::ZERO,
                holder: Box::new(Observed::Unknown(UnknownReason::Inconsistent(
                    "SQLite reported a busy store without a writer gate holder".into(),
                ))),
            },
            // An unreadable file is an environment condition naming its path,
            // never corruption: retrying after the path or its permissions are
            // fixed is safe (§5.5).
            ErrorCode::CannotOpen => io(libc::ENOENT),
            ErrorCode::DiskFull => io(libc::ENOSPC),
            ErrorCode::PermissionDenied | ErrorCode::ReadOnly => io(libc::EACCES),
            _ => Self::Sqlite {
                code: code.extended_code,
                message: error.to_string(),
            },
        }
    }
}
impl From<std::io::Error> for StoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(IoError::new("store", std::path::Path::new(DB_NAME), &error))
    }
}

/// How long one store write waits for the store's writer gate (§5.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BusyWait {
    /// Exactly the caller's remaining operation budget: optional maintenance
    /// and diagnostics that may skip a busy store.
    Context,
    /// At least this window, or the caller's later deadline. A short workload
    /// budget never turns a concurrent writer into an immediate refusal.
    AtLeast(Duration),
}
impl BusyWait {
    fn window(self, cx: &EffectContext) -> Duration {
        let left = cx.operation().remaining();
        let wanted = match self {
            Self::Context => left.unwrap_or(Duration::ZERO),
            Self::AtLeast(minimum) => left.map_or(minimum, |left| left.max(minimum)),
        };
        wanted.min(Duration::from_millis(i32::MAX as u64))
    }
}

/// A writable store handle. Hold one per operation; never across a
/// subprocess, prompt or network call (I-S2).
#[derive(Debug)]
pub struct Store {
    conn: Connection,
    root: StateRoot,
    /// The run this store writes for, for the writer gate's diagnostic body.
    run: Option<RunId>,
}
impl Store {
    /// Opens for writing: creates `kernel.db` 0600, applies the settings in
    /// §5.5, and creates or checks the schema (I-S5).
    pub fn open(cx: &EffectContext, root: &StateRoot) -> Result<Self, StoreError> {
        Self::open_within(cx, root, BusyWait::AtLeast(STORE_BUSY_WAIT))
    }
    /// An open with an explicit wait policy. Optional work (a retention pass)
    /// passes `BusyWait::Context`, so a busy store is skipped, not waited for.
    pub fn open_within(
        cx: &EffectContext,
        root: &StateRoot,
        wait: BusyWait,
    ) -> Result<Self, StoreError> {
        if !root.writable() {
            return Err(StoreError::Untrusted {
                path: root.path().into(),
                reason: "state root opened read-only".into(),
            });
        }
        // A live custodian continuing through a replaced locator is refused.
        let name = FileName::new(root.database_name())?;
        let (_, stat) = root
            .dir()
            .open_file(&name, libc::O_RDWR, Some(FILE_MODE), Trust::Private)?
            .expect("created store file");
        if stat.st_size as u64 > MAX_DB_BYTES {
            return Err(StoreError::TooLarge {
                limit: MAX_DB_BYTES,
            });
        }
        check_store_files(root).map_err(StoreError::from)?;
        let path = root.dir().join(root.database_name());
        let conn = Connection::open_with_flags(
            path.clone(),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|error| match StoreError::from(error) {
            // The path a person needs is the store's own, not `kernel.db`
            // relative to a directory they cannot see from the message.
            StoreError::Io(mut io) => {
                io.path = path;
                StoreError::Io(io)
            }
            other => other,
        })?;
        let mut store = Self {
            conn,
            root: root.clone(),
            run: None,
        };
        store.configure(cx, wait)?;
        Ok(store)
    }
    /// Names the run this store writes for, so a refused writer wait names it.
    pub fn set_run(&mut self, run: RunId) {
        self.run = Some(run);
    }
    /// Write-ahead logging, the durable settings and the schema, under the
    /// store's own writer gate. A current store is only read here: opening it
    /// never takes the write lock (a hot path with many concurrent runs).
    fn configure(&mut self, cx: &EffectContext, wait: BusyWait) -> Result<(), StoreError> {
        let window = wait.window(cx);
        let patient = cx.patience(window);
        let started = Instant::now();
        let _gate = self.writer_gate(&patient, window)?;
        // Configuration PRAGMAs can wait on another connection even when this
        // process already holds the writer gate. Set their SQLite wait before
        // the first statement, rather than inheriting SQLite's five seconds.
        self.conn
            .busy_timeout(window.saturating_sub(started.elapsed()))?;
        // Incremental auto-vacuum must be chosen while the store is empty, and
        // before write-ahead logging materialises its header. On an existing
        // store this is only pending until retention's one-time `VACUUM`.
        self.conn.execute_batch("PRAGMA auto_vacuum=INCREMENTAL")?;
        self.set_journal_mode(&patient, started + window)?;
        self.conn.execute_batch(&format!(
            "PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; \
             PRAGMA cell_size_check=ON; PRAGMA max_page_count={}; \
             PRAGMA journal_size_limit={WAL_LIMIT_BYTES}; PRAGMA wal_autocheckpoint=1000;",
            MAX_DB_BYTES / PAGE_SIZE
        ))?;
        self.schema(&patient, BusyWait::Context)?;
        Ok(())
    }
    /// `journal_mode=WAL` is persistent in the file. A store last written in
    /// rollback mode needs a moment of exclusive access to change it, so a
    /// concurrent open is retried inside this write's own window.
    fn set_journal_mode(&self, cx: &EffectContext, deadline: Instant) -> Result<(), StoreError> {
        let mut pause = BUSY_RETRY_MIN;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            self.conn
                .busy_timeout(left.min(Duration::from_millis(i32::MAX as u64)))?;
            match self
                .conn
                .query_row("PRAGMA journal_mode=WAL", [], |row| row.get::<_, String>(0))
            {
                Ok(mode) if mode.eq_ignore_ascii_case("wal") => return Ok(()),
                // Another connection keeps the old mode until it closes.
                Ok(_) if !left.is_zero() => {}
                Ok(mode) => {
                    // The file is readable; only its mode differs from what
                    // Effigy sets, so this is an invariant failure and never
                    // corruption that moves the store aside (effigy#360).
                    return Err(StoreError::Broken(format!(
                        "kernel.db runs in journal mode {mode} instead of the write-ahead log \
                         Effigy sets"
                    )));
                }
                Err(error) => match StoreError::from(error) {
                    StoreError::Busy { .. } if !left.is_zero() => {}
                    other => return Err(other),
                },
            }
            if !left.is_zero() {
                cx.operation()
                    .check("store journal mode")
                    .map_err(StoreError::Interrupted)?;
                std::thread::sleep(pause.min(left));
                pause = (pause * 2).min(BUSY_RETRY_MAX);
            }
        }
    }
    /// Read-only open (R2): one plain SQLite read, no copy. A checkpointed
    /// store is opened immutable; a store with write-ahead side files is read
    /// through SQLite's own WAL. Nothing is created, locked for writing or
    /// repaired, and an unreadable journal is `Unknown` for the caller to
    /// warn about (K1).
    pub fn read(cx: &ObserveContext, root: &StateRoot) -> Observed<StoreReader> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        // A live custodian continuing through a replaced locator is refused.
        if let Err(error) = check_store_files(root) {
            return Observed::Unknown(crate::root::unknown(&error));
        }
        let name = FileName::new(root.database_name()).expect("fixed name");
        match root
            .dir()
            .open_file(&name, libc::O_RDONLY, None, Trust::Private)
        {
            Ok(Some((_, stat))) if stat.st_size as u64 > MAX_DB_BYTES => {
                return Observed::Unknown(UnknownReason::StoreUnreadable("oversized".into()))
            }
            Ok(Some(_)) => {}
            Ok(None) => return Observed::Absent(AbsenceProof::NoStore),
            Err(error) => return Observed::Unknown(crate::root::unknown(&error)),
        }
        let opened = open_reading(root, cx.remaining().min(READ_BUSY_WAIT));
        match opened {
            Ok(Some(reader)) => Observed::Verified(reader, evidence("kernel store")),
            Ok(None) => Observed::Absent(AbsenceProof::NoStore),
            Err(error) => Observed::Unknown(read_unknown(&error)),
        }
    }
    /// One `BEGIN IMMEDIATE` transaction. The closure's records commit
    /// together or not at all (I-S1). The write waits for the store's own
    /// writer window, never just for the caller's remaining budget.
    pub fn txn<T>(
        &mut self,
        cx: &EffectContext,
        f: impl FnOnce(&mut Txn<'_>) -> Result<T, StoreError>,
    ) -> Result<T, StoreError> {
        self.transaction(cx, f)
    }
    /// Kernel primitives keep typed refusals inside the same atomic transaction.
    pub(crate) fn transaction<T, E: From<StoreError>>(
        &mut self,
        cx: &EffectContext,
        f: impl FnOnce(&mut Txn<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        self.transaction_within(cx, BusyWait::AtLeast(STORE_BUSY_WAIT), f)
    }
    /// A write with an explicit wait policy. Settle and outcome writes pass
    /// `SETTLE_BUSY_WAIT`: they keep trying after the workload's own deadline
    /// has passed, so a busy store never costs the run its outcome (K4).
    pub fn transaction_within<T, E: From<StoreError>>(
        &mut self,
        cx: &EffectContext,
        wait: BusyWait,
        f: impl FnOnce(&mut Txn<'_>) -> Result<T, E>,
    ) -> Result<T, E> {
        let window = wait.window(cx);
        // Cancellation still wins; only the deadline is replaced.
        let patient = cx.patience(window);
        patient
            .operation()
            .check("store transaction")
            .map_err(StoreError::Interrupted)?;
        let deadline = Instant::now() + window;
        let _gate = self.writer_gate(&patient, window)?;
        let flag = TxnFlag::enter()?;
        self.begin_immediate(&patient, deadline)?;
        let value = f(&mut Txn {
            conn: &self.conn,
            now: wall_ms(),
            machine: self.root.is_machine(),
        });
        let value = match value {
            Ok(value) => match crate::fault_point!(StoreCommitBefore) {
                Ok(()) => Ok(value),
                Err(error) => Err(StoreError::from(error).into()),
            },
            Err(error) => Err(error),
        };
        let committed = match value {
            Ok(value) => match self.conn.execute_batch("COMMIT") {
                Ok(()) => Ok(value),
                Err(error) => {
                    // A commit that did not return is rolled back: the
                    // closure's records never land half-written (I-S1).
                    let _ = self.conn.execute_batch("ROLLBACK");
                    Err(StoreError::from(error).into())
                }
            },
            Err(error) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                Err(error)
            }
        };
        drop(flag);
        let value = committed?;
        crate::fault_point!(StoreCommitAfter).map_err(StoreError::from)?;
        Ok(value)
    }
    /// The store's own writer lock (§5.5). It serializes Effigy's writers, so
    /// a refusal names the process and run that still holds the store. A
    /// caller that already holds it in this process (`Store::repair`) writes
    /// without taking it twice: a second flock would wait on itself.
    fn writer_gate(
        &self,
        cx: &EffectContext,
        window: Duration,
    ) -> Result<Option<GateGuard>, StoreError> {
        let id = self.root.store_gate(cx, WRITER_GATE)?;
        let name = |guard: GateGuard| {
            if let Some(run) = &self.run {
                guard.set_run(run);
            }
            guard
        };
        match Gate::acquire(cx, &self.root, std::slice::from_ref(&id), Wait::FailFast) {
            Ok(guard) => Ok(Some(name(guard))),
            Err(GateError::Busy { holder, .. }) | Err(GateError::TimedOut { holder, .. })
                if holds_here(&holder) =>
            {
                Ok(None)
            }
            Err(GateError::Busy { .. }) => {
                let deadline = Instant::now() + window;
                match Gate::acquire(cx, &self.root, &[id], Wait::Until(deadline)) {
                    Ok(guard) => Ok(Some(name(guard))),
                    Err(error) if holds_here(&holder_of(&error)) => Ok(None),
                    Err(error) => Err(busy(error)),
                }
            }
            Err(error) => Err(error.into()),
        }
    }
    fn begin_immediate(&self, cx: &EffectContext, deadline: Instant) -> Result<(), StoreError> {
        let mut pause = BUSY_RETRY_MIN;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            self.conn
                .busy_timeout(left.min(Duration::from_millis(i32::MAX as u64)))?;
            match self.conn.execute_batch("BEGIN IMMEDIATE") {
                Ok(()) => return Ok(()),
                Err(error) => match StoreError::from(error) {
                    StoreError::Busy { .. } if !left.is_zero() => {}
                    other => return Err(other),
                },
            }
            if left.is_zero() {
                return Err(StoreError::Busy {
                    waited: Duration::ZERO,
                    holder: Box::new(Observed::Unknown(UnknownReason::Inconsistent(
                        "SQLite reported a busy store without a writer gate holder".into(),
                    ))),
                });
            }
            cx.operation()
                .check("store transaction")
                .map_err(StoreError::Interrupted)?;
            std::thread::sleep(pause.min(left));
            pause = (pause * 2).min(BUSY_RETRY_MAX);
        }
    }
    /// The store file's size on disk, from the open connection.
    fn db_bytes(&self, page_size: i64) -> Result<u64, StoreError> {
        let pages: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        Ok(pages.max(0) as u64 * page_size.max(0) as u64)
    }
    /// Bounded maintenance (§7.4): checkpoint the log, migrate a *small* old
    /// store to incremental auto-vacuum once, then release at most `pages`
    /// free pages. It never rewrites a large store.
    pub fn maintain(
        &mut self,
        cx: &EffectContext,
        pages: u32,
    ) -> Result<MaintainReport, StoreError> {
        let started = Instant::now();
        let mut report = MaintainReport::default();
        let window = BusyWait::AtLeast(STORE_BUSY_WAIT).window(cx);
        let patient = cx.patience(window);
        let _gate = self.writer_gate(&patient, window)?;
        cx.operation()
            .check("store maintenance")
            .map_err(StoreError::Interrupted)?;
        // Never blocks a reader; the log is truncated at the next checkpoint.
        let (_busy, _log, checkpointed): (i64, i64, i64) =
            self.conn
                .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        report.checkpointed = checkpointed as u32;
        let mode: i64 = self
            .conn
            .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))?;
        let migrated: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key='auto_vacuum' AND value='incremental')",
            [],
            |row| row.get(0),
        )?;
        let page_size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        if mode == 0 && !migrated && legacy_vacuum_allowed(self.db_bytes(page_size)?) {
            // One-time migration of a *small* store created before incremental
            // auto-vacuum: only `VACUUM` applies it, and a sweep must never
            // rewrite a large legacy store inside its write window. A larger
            // one keeps its high-water mark and reuses freed pages instead.
            let free: i64 = self
                .conn
                .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
            if free > 0 {
                self.conn
                    .execute_batch("PRAGMA auto_vacuum=INCREMENTAL; VACUUM;")?;
                self.conn.execute(
                    "INSERT OR REPLACE INTO meta(key, value) VALUES ('auto_vacuum', 'incremental')",
                    [],
                )?;
                report.migrated_auto_vacuum = true;
            }
        }
        let vacuum: i64 = self
            .conn
            .query_row("PRAGMA auto_vacuum", [], |row| row.get(0))?;
        let free: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        report.free_before = free as u32;
        report.auto_vacuum = vacuum;
        let take = i64::from(pages).min(free);
        if vacuum == 2 && take > 0 {
            self.conn
                .execute_batch(&format!("PRAGMA incremental_vacuum({take})"))?;
            report.freed_pages = take as u32;
        }
        report.free_after = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get::<_, i64>(0))?
            as u32;
        report.elapsed_ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        Ok(report)
    }
    /// O6: integrity and foreign-key checks.
    pub fn check_integrity(&self) -> Result<(), StoreError> {
        let integrity: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Corrupt(format!(
                "PRAGMA integrity_check: {integrity}"
            )));
        }
        let violations: i64 =
            self.conn
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
        if violations != 0 {
            return Err(StoreError::Corrupt(format!(
                "{violations} foreign-key violations"
            )));
        }
        Ok(())
    }
    /// The explicit repair: moves an unreadable `kernel.db` aside so the next
    /// open starts empty. Resources it described become `Unknown` (R3).
    pub fn repair(cx: &EffectContext, root: &StateRoot) -> Result<FileName, StoreError> {
        if cx.operation().class() != EffectClass::Mutate {
            return Err(StoreError::ClassRefused(cx.operation().class()));
        }
        let gate = root.store_gate(cx, WRITER_GATE)?;
        let _guard = Gate::acquire(cx, root, &[gate], Wait::FailFast)?;
        match Self::open(cx, root) {
            Ok(_) => return Err(StoreError::NotCorrupt),
            // A store this Effigy cannot read at all, corrupt or of an unknown
            // schema, is what the repair is for.
            Err(StoreError::Corrupt(_)) | Err(StoreError::UnknownSchema { .. }) => {}
            Err(other) => return Err(other),
        }
        let stamp = wall_ms();
        let aside = FileName::new(&format!("{}.corrupt-{stamp}", root.database_name()))?;
        // The journal and the write-ahead log and index are the store's own
        // files: they move with it, or the next `kernel.db` inherits a foreign
        // log written by the repaired one (§5.5).
        for suffix in ["-journal", "-wal", "-shm"] {
            let side = FileName::new(&format!("{}{suffix}", root.database_name()))?;
            if root.dir().entry(&side)?.is_some() {
                root.dir().rename(&side, &aside.with_suffix(suffix)?)?;
            }
        }
        root.dir()
            .rename(&FileName::new(root.database_name())?, &aside)?;
        root.dir().sync()?;
        Ok(aside)
    }
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
    fn schema(&mut self, cx: &EffectContext, wait: BusyWait) -> Result<(), StoreError> {
        // Pure read first: a current store needs no write transaction, and so
        // never takes the write lock just to be opened (I-S5, §5.5).
        let tables: i64 = self.conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table'",
            [],
            |row| row.get(0),
        )?;
        if tables == 0 {
            // Incremental auto-vacuum must be chosen before the first table.
            // A fresh store gets the additive tables and indexes too: the
            // constants describe one complete schema, and the very first write
            // may already be a newer primitive (a workspace activation
            // reservation, for instance).
            self.conn.execute_batch("PRAGMA auto_vacuum=INCREMENTAL")?;
            return self.transaction_within(cx, wait, |txn| {
                txn.conn.execute_batch(SCHEMA_V1)?;
                for (_, sql) in ADDITIVE_TABLES {
                    txn.conn.execute_batch(sql)?;
                }
                for (_, sql) in ADDITIVE_INDEXES {
                    txn.conn.execute_batch(sql)?;
                }
                txn.conn.execute(
                    "INSERT INTO meta(key, value) VALUES ('schema_version', ?1), \
                     ('created_by', ?2), ('last_written_by', ?2), \
                     ('auto_vacuum', 'incremental')",
                    params![SCHEMA_VERSION.to_string(), BUILD],
                )?;
                Ok(())
            });
        }
        let found = schema_version(&self.conn)?;
        if found != SCHEMA_VERSION {
            return Err(StoreError::UnknownSchema { found });
        }
        let stale_build: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key='last_written_by' AND value<>?1)",
            [BUILD],
            |row| row.get(0),
        )?;
        let adds_index = ADDITIVE_INDEXES
            .iter()
            .any(|(name, _)| !has_index(&self.conn, name).unwrap_or(false));
        let adds_table = ADDITIVE_TABLES
            .iter()
            .any(|(name, _)| !has_table(&self.conn, name).unwrap_or(false));
        if has_backend(&self.conn)?
            && has_column(&self.conn, "engine")?
            && has_session_owner(&self.conn)?
            && has_activation_session_owner(&self.conn)?
            && !stale_build
            && !adds_index
            && !adds_table
        {
            return Ok(());
        }
        // Schema 1 additive changes: old readers select explicit columns;
        // Observe reads an older store without migrating it.
        self.transaction_within(cx, wait, |txn| {
            if !has_backend(txn.conn)? {
                txn.conn
                    .execute_batch("ALTER TABLE resources ADD COLUMN backend TEXT")?;
            }
            if !has_column(txn.conn, "engine")? {
                txn.conn
                    .execute_batch("ALTER TABLE resources ADD COLUMN engine TEXT")?;
            }
            for (name, sql) in ADDITIVE_TABLES {
                if !has_table(txn.conn, name)? {
                    txn.conn.execute_batch(sql)?;
                }
            }
            if !has_activation_session_owner(txn.conn)? {
                txn.conn.execute_batch("ALTER TABLE workspace_activation_reservations ADD COLUMN session_owner TEXT;
                    UPDATE workspace_activation_reservations SET session_owner=(SELECT owner FROM runs WHERE run_id=session_run)
                    WHERE session_run IS NOT NULL")?;
            }
            if !has_session_owner(txn.conn)? {
                txn.conn.execute_batch(
                    "ALTER TABLE workspace_admissions ADD COLUMN session_owner TEXT",
                )?;
            }
            for (name, sql) in ADDITIVE_INDEXES {
                if !has_index(txn.conn, name)? {
                    txn.conn.execute_batch(sql)?;
                }
            }
            txn.conn.execute(
                "UPDATE meta SET value=?1 WHERE key='last_written_by' AND value<>?1",
                params![BUILD],
            )?;
            Ok(())
        })
    }
}

/// What one bounded maintenance pass did (§7.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MaintainReport {
    /// Free pages the write-ahead log checkpoint carried into the store.
    pub checkpointed: u32,
    /// A store created before incremental auto-vacuum was migrated once.
    pub migrated_auto_vacuum: bool,
    /// Free pages released to the filesystem by incremental vacuum.
    pub freed_pages: u32,
    /// Free pages the vacuum pass saw, and those left afterwards.
    pub free_before: u32,
    pub free_after: u32,
    /// The auto-vacuum mode the pass observed.
    pub auto_vacuum: i64,
    /// Wall time this pass spent inside maintenance.
    pub elapsed_ms: u32,
}

/// Typed operations inside one transaction. Not `Send`: it cannot leave the
/// thread that opened it.
pub struct Txn<'a> {
    conn: &'a Connection,
    now: i64,
    machine: bool,
}
impl std::fmt::Debug for Txn<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Txn")
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResourceRecord {
    pub name: String,
    pub label: String,
    pub authority: String,
    pub generation: Generation,
    pub state: Option<String>,
    pub backend: Option<BackendMetadata>,
    /// The authority engine's own record, opaque to the kernel and written
    /// only by that engine's runs under the resource gate.
    pub engine: Option<serde_json::Value>,
}
/// One immutable-object reference of a resource (§7.4): its role and order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ObjectRef {
    pub role: String,
    pub digest: String,
    pub seq: i64,
}
/// Exact engine/context identity and the policy source that selected it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendMetadata {
    pub engine: String,
    pub context: String,
    pub source: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CheckoutState {
    Live,
    Tombstoned,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckoutRecord {
    pub token: String,
    pub path_hint: Option<String>,
    pub git_dir_hint: Option<String>,
    pub state: CheckoutState,
}
impl Txn<'_> {
    /// Inserts the resource row or verifies its canonical bytes (I-ID2).
    pub fn resource(
        &mut self,
        id: &ResourceId,
        authority: &str,
    ) -> Result<ResourceRecord, StoreError> {
        if self.machine && !id.is_machine_fact() {
            return Err(StoreError::Refused(format!(
                "{} belongs to a checkout journal, not the shared machine store",
                id.label()
            )));
        }
        let name = id.name().to_string();
        let existing: Option<Vec<u8>> = self
            .conn
            .query_row(
                "SELECT canonical FROM resources WHERE name=?1",
                [&name],
                |row| row.get(0),
            )
            .optional()?;
        match existing {
            Some(existing) => id.check_existing(&existing)?,
            None => {
                let checkout = match id.scope() {
                    Scope::Machine => None,
                    Scope::Checkout(checkout) => {
                        self.checkout_seen(&checkout, None, None)?;
                        Some(token(&checkout))
                    }
                };
                self.conn.execute(
                    "INSERT INTO resources(name, canonical, kind, scope, checkout, label, \
                     authority, generation, updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,0,?8)",
                    params![
                        name,
                        id.canonical(),
                        id.kind() as u8,
                        gate::scope_name(id.scope()),
                        checkout,
                        id.label(),
                        authority,
                        self.now
                    ],
                )?;
            }
        }
        Ok(self
            .conn
            .query_row(&resource_sql(self.conn)?, [&name], resource_row)?)
    }
    /// `generation += 1` (K7). Overflow is refused, never wrapped.
    pub fn next_generation(&mut self, id: &ResourceId) -> Result<Generation, StoreError> {
        let current: i64 = self
            .conn
            .query_row(
                "SELECT generation FROM resources WHERE name=?1",
                [id.name().to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(StoreError::UnknownResource)?;
        let current = Generation::new(current as u64);
        let next = current
            .next()
            .ok_or(StoreError::Identity(IdentityError::TooLong))?;
        self.conn.execute(
            "UPDATE resources SET generation=?1, updated_at=?2 WHERE name=?3",
            params![next.get() as i64, self.now, id.name().to_string()],
        )?;
        Ok(next)
    }
    pub fn checkout_seen(
        &mut self,
        checkout: &CheckoutId,
        path_hint: Option<&str>,
        git_dir_hint: Option<&str>,
    ) -> Result<(), StoreError> {
        if self.machine {
            return Err(StoreError::Refused(
                "the checkout catalog belongs to a checkout journal, not the shared machine store"
                    .into(),
            ));
        }
        self.conn.execute(
            "INSERT INTO checkouts(token, kind, path_hint, git_dir_hint, state, first_seen, last_seen) \
             VALUES (?1, ?2, ?3, ?4, 'live', ?5, ?5) \
             ON CONFLICT(token) DO UPDATE SET last_seen=?5, \
               path_hint=coalesce(?3, path_hint), git_dir_hint=coalesce(?4, git_dir_hint)",
            params![
                token(checkout),
                checkout.kind() as u8,
                path_hint,
                git_dir_hint,
                self.now
            ],
        )?;
        Ok(())
    }
    /// Called after a Mutate observed `Verified` absence of the checkout's
    /// path and token (§7.4). Returns false when no row exists.
    pub fn checkout_tombstone(&mut self, checkout: &CheckoutId) -> Result<bool, StoreError> {
        Ok(self.conn.execute(
            "UPDATE checkouts SET state='tombstoned', tombstoned_at=coalesce(tombstoned_at, ?2) \
             WHERE token=?1",
            params![token(checkout), self.now],
        )? == 1)
    }
    pub(crate) fn conn(&self) -> &Connection {
        self.conn
    }
    pub(crate) fn now(&self) -> i64 {
        self.now
    }
}

/// A read-only store view. Reads only the columns schema 1 defines (I-S5).
#[derive(Debug)]
/// One coherent observation. Reopen for facts after a subsequent mutation;
/// no filesystem SQLite connection or read lock survives this snapshot.
pub struct StoreReader {
    conn: Connection,
    version: i64,
}
impl StoreReader {
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
    pub fn schema_version(&self) -> i64 {
        self.version
    }
    pub fn last_written_by(&self) -> Observed<String> {
        self.row(
            "meta",
            "SELECT value FROM meta WHERE key='last_written_by'",
            [],
            |row| row.get(0),
        )
    }
    /// An immutable local import alias. The referenced canonical resource must
    /// still exist; a missing row is corruption, never a fresh owner.
    pub fn imported_owner(&self, role: &str) -> Observed<ResourceId> {
        let name: Observed<String> = self.row(
            "meta",
            "SELECT value FROM meta WHERE key=?1",
            [format!("migration_owner:{role}")],
            |r| r.get(0),
        );
        let name = match name {
            Observed::Verified(name, _) => name,
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        };
        let canonical: Observed<Vec<u8>> = self.row(
            "resources",
            "SELECT canonical FROM resources WHERE name=?1",
            [&name],
            |r| r.get(0),
        );
        match canonical {
            Observed::Verified(bytes, evidence) => match ResourceId::decode_canonical(&bytes) {
                Ok(id) if id.name().to_string() == name => Observed::Verified(id, evidence),
                _ => Observed::Unknown(UnknownReason::Inconsistent(
                    "invalid imported owner alias".into(),
                )),
            },
            Observed::Unknown(reason) => Observed::Unknown(reason),
            Observed::Absent(_) => Observed::Unknown(UnknownReason::Inconsistent(
                "imported owner alias has no resource".into(),
            )),
        }
    }
    pub fn resource(&self, id: &ResourceId) -> Observed<ResourceRecord> {
        let canonical: Observed<Vec<u8>> = self.row(
            "resources",
            "SELECT canonical FROM resources WHERE name=?1",
            [id.name().to_string()],
            |row| row.get(0),
        );
        match canonical {
            Observed::Verified(existing, _) => {
                if id.check_existing(&existing).is_err() {
                    return Observed::Unknown(UnknownReason::Inconsistent(
                        "resource name collision".into(),
                    ));
                }
            }
            Observed::Absent(proof) => return Observed::Absent(proof),
            Observed::Unknown(reason) => return Observed::Unknown(reason),
        }
        let sql = match resource_sql(&self.conn) {
            Ok(sql) => sql,
            Err(error) => return Observed::Unknown(read_unknown(&error.into())),
        };
        self.row("resources", &sql, [id.name().to_string()], resource_row)
    }
    pub fn checkout(&self, checkout: &CheckoutId) -> Observed<CheckoutRecord> {
        self.row(
            "checkouts",
            "SELECT token, path_hint, git_dir_hint, state FROM checkouts WHERE token=?1",
            [token(checkout)],
            |row| {
                Ok(CheckoutRecord {
                    token: row.get(0)?,
                    path_hint: row.get(1)?,
                    git_dir_hint: row.get(2)?,
                    state: if row.get::<_, String>(3)? == "tombstoned" {
                        CheckoutState::Tombstoned
                    } else {
                        CheckoutState::Live
                    },
                })
            },
        )
    }
    /// Every object reference of one resource, oldest first. Pure read: no
    /// closure is opened, so a missing object still reads as its row.
    pub fn object_refs(&self, id: &ResourceId) -> Observed<Vec<ObjectRef>> {
        let result = (|| -> rusqlite::Result<Vec<ObjectRef>> {
            let mut stmt = self.conn.prepare(
                "SELECT role,digest,seq FROM object_refs WHERE resource=?1 ORDER BY seq,role",
            )?;
            let rows = stmt
                .query_map([id.name().to_string()], |row| {
                    Ok(ObjectRef {
                        role: row.get(0)?,
                        digest: row.get(1)?,
                        seq: row.get(2)?,
                    })
                })?
                .collect();
            rows
        })();
        match result {
            Ok(rows) => Observed::Verified(rows, evidence("kernel object references")),
            Err(error) => Observed::Unknown(read_unknown(&error.into())),
        }
    }
    /// The latest operation for one canonical owner. A missing row is not
    /// proof of process absence; callers also observe its gate and custody.
    pub fn latest_run(&self, owner: &ResourceId) -> Observed<(RunId, Generation)> {
        let row: Observed<(String, i64)> = self.row(
            "runs",
            "SELECT run_id,generation FROM runs WHERE owner=?1 ORDER BY generation DESC LIMIT 1",
            [owner.name().to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        );
        match row {
            Observed::Verified((id, generation), evidence) => match id.parse() {
                Ok(id) if generation > 0 => {
                    Observed::Verified((id, Generation::new(generation as u64)), evidence)
                }
                _ => Observed::Unknown(UnknownReason::StoreUnreadable(
                    "invalid run identity".into(),
                )),
            },
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }
    pub fn run_phase(&self, run: &RunId) -> Observed<RunPhaseFact> {
        self.row(
            "runs",
            "SELECT phase FROM runs WHERE run_id=?1",
            [run.to_string()],
            |row| {
                Ok(if row.get::<_, String>(0)? == "closed" {
                    RunPhaseFact::Closed
                } else {
                    RunPhaseFact::NonClosed
                })
            },
        )
    }
    /// D4 status projection: the run row plus a fresh owner-gate probe.
    pub fn run_owner_state(
        &self,
        cx: &ObserveContext,
        root: &StateRoot,
        run: &RunId,
    ) -> Observed<OwnerState> {
        let phase = self.run_phase(run);
        let gate = match &phase {
            Observed::Verified(RunPhaseFact::NonClosed, _) => {
                let sql = match has_table(&self.conn, "workspace_session_runs") {
                    Ok(true) => "SELECT r.scope, coalesce(s.gate_key, r.name) FROM runs \
                        JOIN resources r ON r.name=runs.owner \
                        LEFT JOIN workspace_session_runs s ON s.run_id=runs.run_id WHERE runs.run_id=?1",
                    Ok(false) => "SELECT r.scope,r.name FROM runs JOIN resources r ON r.name=runs.owner WHERE runs.run_id=?1",
                    Err(error) => return Observed::Unknown(read_unknown(&error.into())),
                };
                let owner: Observed<(String, String)> =
                    self.row("resources", sql, [run.to_string()], |row| {
                        Ok((row.get(0)?, row.get(1)?))
                    });
                match owner {
                    Observed::Verified((scope, name), _) => {
                        gate::probe_names(cx, root, &scope, &name)
                    }
                    Observed::Absent(_) => Observed::Unknown(UnknownReason::Inconsistent(
                        "run without owner resource".into(),
                    )),
                    Observed::Unknown(reason) => Observed::Unknown(reason),
                }
            }
            _ => Observed::Absent(AbsenceProof::NoGateInode),
        };
        gate::owner_state(phase, gate)
    }
    fn row<T, P: rusqlite::Params>(
        &self,
        table: &str,
        sql: &str,
        params: P,
        map: impl FnOnce(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    ) -> Observed<T> {
        match self.conn.query_row(sql, params, map).optional() {
            Ok(Some(value)) => Observed::Verified(value, evidence("kernel store row")),
            Ok(None) => Observed::Absent(AbsenceProof::NoRow {
                table: table.into(),
            }),
            Err(error) => Observed::Unknown(read_unknown(&error.into())),
        }
    }
}

/// A persisted deadline (§7.1): boot identity plus boot-monotonic time.
/// A different boot ends it; wall time is never consulted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BootDeadline {
    pub boot: String,
    pub nanos: u64,
}
impl BootDeadline {
    pub fn after(now: &BootInstant, budget: Duration) -> Self {
        Self {
            boot: now.boot.as_str().into(),
            nanos: u64::try_from(now.elapsed.saturating_add(budget).as_nanos()).unwrap_or(u64::MAX),
        }
    }
    pub fn passed(&self, now: &Observed<BootInstant>) -> Observed<bool> {
        match now {
            Observed::Verified(now, _) => Observed::Verified(
                now.boot.as_str() != self.boot
                    || u64::try_from(now.elapsed.as_nanos()).unwrap_or(u64::MAX) >= self.nanos,
                evidence("kernel boot clock"),
            ),
            Observed::Absent(proof) => Observed::Absent(proof.clone()),
            Observed::Unknown(reason) => Observed::Unknown(reason.clone()),
        }
    }
}

pub(crate) fn wall_ms() -> i64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_millis() as i64)
        .unwrap_or(0);
    // ClockJump (§12.1): only diagnostics and retention read the wall clock.
    #[cfg(all(test, feature = "fault-injection"))]
    let now = now + crate::fault::clock_skew_ms();
    now
}
pub(crate) fn token(checkout: &CheckoutId) -> String {
    checkout
        .token()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn evidence(source: &str) -> Evidence {
    Evidence::new(source.into(), None).expect("fixed source")
}
/// The gate diagnostic body names the writer's pid and, when it writes for a
/// run, its run id (§5.4, I-G2).
fn holder_note(holder: &Observed<HolderDiag>) -> String {
    match holder.verified() {
        Some(diag) => match &diag.run {
            Some(run) => format!(" (pid {} of run {run})", diag.pid),
            None => format!(" (pid {} outside any run)", diag.pid),
        },
        None => String::new(),
    }
}
/// Whether a gate diagnostic names this process: an outer gate in the same
/// process (the explicit repair) is already ours, so writing again must not
/// wait for it.
fn holds_here(holder: &Observed<HolderDiag>) -> bool {
    holder
        .verified()
        .is_some_and(|diag| diag.pid == std::process::id())
}
fn holder_of(error: &GateError) -> Observed<HolderDiag> {
    match error {
        GateError::Busy { holder, .. } | GateError::TimedOut { holder, .. } => (**holder).clone(),
        _ => Observed::Unknown(UnknownReason::Inconsistent(
            "not a store writer wait".into(),
        )),
    }
}
fn busy(error: GateError) -> StoreError {
    match error {
        GateError::Busy { holder, .. } => StoreError::Busy {
            waited: Duration::ZERO,
            holder,
        },
        GateError::TimedOut { waited, holder, .. } => StoreError::Busy { waited, holder },
        other => other.into(),
    }
}
/// The store's own files: the database and, during commits, its write-ahead
/// log and shared-memory index. Each one must carry the private-file trust
/// (§7.1), including the log and index a WAL store keeps (§5.5).
fn check_store_files(root: &StateRoot) -> Result<(), FsError> {
    for suffix in ["", "-journal", "-wal", "-shm"] {
        let name = FileName::new(&format!("{}{suffix}", root.database_name()))?;
        if let Some(stat) = root.dir().entry(&name)? {
            crate::fs::check(
                &stat,
                Trust::Private,
                crate::fs::Kind::File,
                &root.dir().join(name.as_str()),
            )?;
        }
    }
    Ok(())
}
fn verify_reader(conn: Connection, busy: Duration) -> Result<Option<StoreReader>, StoreError> {
    conn.busy_timeout(busy)?;
    conn.execute_batch("PRAGMA query_only=1; PRAGMA trusted_schema=OFF;")?;
    // A writer creates the file before its schema transaction commits: no
    // schema object yet is a store still being created, with nothing in it.
    let objects: i64 =
        conn.query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))?;
    if objects == 0 {
        return Ok(None);
    }
    let version = schema_version(&conn)?;
    Ok(Some(StoreReader { conn, version }))
}
/// Open the store for reading without copying it: no lock of Effigy's own,
/// no file created and nothing repaired. WAL already gives one consistent
/// snapshot, so a writer beside this read is not an obstacle.
///
/// A store with a hot write-ahead log, or the index beside it, is a plain
/// read-only connection to SQLite's own WAL: its committed frames are visible
/// and a crashed writer's are not. A checkpointed store is an immutable open —
/// no lock, no index and no file — which is also the only form that writes
/// nothing into a read-only or copied checkout (K1, Q-011).
fn open_reading(root: &StateRoot, busy: Duration) -> Result<Option<StoreReader>, StoreError> {
    // A state root whose database is not created yet holds nothing, whatever
    // its side files say.
    if root
        .dir()
        .entry(&FileName::new(root.database_name())?)?
        .is_none()
    {
        return Ok(None);
    }
    let wal = FileName::new(&format!("{}-wal", root.database_name()))?;
    let index = FileName::new(&format!("{}-shm", root.database_name()))?;
    let hot = root.dir().entry(&index)?.is_some()
        || root.dir().entry(&wal)?.is_some_and(|stat| stat.st_size > 0);
    // The plain read-only open takes the write-ahead index writable, so a
    // writer in this process still gets a writable one. `readonly_shm` left
    // the index read-only and the next write failed (K1 follow-up).
    let query = if hot { "mode=ro" } else { "immutable=1" };
    let conn = Connection::open_with_flags(
        format!(
            "file:{}?{query}",
            uri_path(root.path().join(root.database_name()))
        ),
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NOFOLLOW
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    verify_reader(conn, busy)
}

/// The largest store a sweep may migrate to incremental auto-vacuum in one
/// `VACUUM`. Above it the rewrite would hold the write lock for seconds on a
/// store the operator is protecting; the store keeps its high-water mark and
/// reuses freed pages, which keeps it proportionate (§7.4).
pub(crate) const LEGACY_VACUUM_MAX_BYTES: u64 = 16 << 20;

/// Whether a sweep may apply the one-time auto-vacuum migration to a store of
/// this size. The rewrite is proportional to the file, so only a small file
/// qualifies.
pub(crate) fn legacy_vacuum_allowed(bytes: u64) -> bool {
    bytes <= LEGACY_VACUUM_MAX_BYTES
}

/// Percent-escapes the characters a SQLite URI reserves.
fn uri_path(path: PathBuf) -> String {
    let path = path.as_os_str().as_bytes();
    let mut out = String::with_capacity(path.len());
    for &byte in path {
        if byte.is_ascii_alphanumeric() || b"/._-~:".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}
fn schema_version(conn: &Connection) -> Result<i64, StoreError> {
    // A query that cannot be answered is the query's own error (busy, locked,
    // cannot open, permission); only a store with no readable schema version at
    // all is corrupt.
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(StoreError::from)?;
    value
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| StoreError::Corrupt("it has no schema version".into()))
}
fn read_unknown(error: &StoreError) -> UnknownReason {
    match error {
        StoreError::Untrusted { path, reason } if reason == crate::fs::OPEN_MODE_REASON => {
            UnknownReason::Untrusted(format!(
                "{} is readable by other users; run `chmod 700 {}`",
                path.display(),
                path.display()
            ))
        }
        StoreError::Untrusted { path, reason } => UnknownReason::Untrusted(format!(
            "{} is not trusted store state ({reason}); fix its ownership and permissions",
            path.display()
        )),
        StoreError::Busy { .. } => UnknownReason::Busy {
            phase: "store read".into(),
        },
        StoreError::Io(io) if io.is_permission() => UnknownReason::PermissionDenied,
        other => UnknownReason::StoreUnreadable(other.to_string()),
    }
}
fn has_session_owner(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('workspace_admissions') WHERE name='session_owner')",
        [], |r| r.get(0),
    )
}
fn has_backend(conn: &Connection) -> rusqlite::Result<bool> {
    has_column(conn, "backend")
}
fn has_column(conn: &Connection, column: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('resources') WHERE name=?1)",
        [column],
        |r| r.get(0),
    )
}
fn has_activation_session_owner(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('workspace_activation_reservations') WHERE name='session_owner')",
        [], |row| row.get(0))
}
pub(crate) fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [name],
        |r| r.get(0),
    )
}
fn has_index(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?1)",
        [name],
        |r| r.get(0),
    )
}
fn resource_sql(conn: &Connection) -> rusqlite::Result<String> {
    let backend = if has_backend(conn)? {
        "backend"
    } else {
        "NULL"
    };
    let engine = if has_column(conn, "engine")? {
        "engine"
    } else {
        "NULL"
    };
    Ok(format!(
        "SELECT name,label,authority,generation,state,{backend},{engine} FROM resources WHERE name=?1"
    ))
}
fn resource_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResourceRecord> {
    Ok(ResourceRecord {
        name: row.get(0)?,
        label: row.get(1)?,
        authority: row.get(2)?,
        generation: Generation::new(row.get::<_, i64>(3)? as u64),
        state: row.get(4)?,
        backend: row
            .get::<_, Option<String>>(5)?
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        5,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?,
        engine: row
            .get::<_, Option<String>>(6)?
            .map(|value| {
                serde_json::from_str(&value).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        6,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })
            })
            .transpose()?,
    })
}

/// Additive schema-1 tables: an old reader selects explicit columns from the
/// tables it knows, so a new table is invisible to it and needs no version
/// bump. A new table is still created here, in the same bounded transaction
/// that adds the additive indexes.
const ADDITIVE_TABLES: [(&str, &str); 6] = [
    (
        "foreground_terminals",
        "CREATE TABLE IF NOT EXISTS foreground_terminals (
  run_id TEXT PRIMARY KEY,
  checkout TEXT NOT NULL,
  boot TEXT NOT NULL,
  task TEXT NOT NULL,
  outcome TEXT NOT NULL,
  exit_code INTEGER,
  signal INTEGER,
  cleanup TEXT NOT NULL,
  workload_groups INTEGER NOT NULL,
  closed_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS foreground_terminals_closed
  ON foreground_terminals(closed_at)",
    ),
    // 052 §8.11: one keyed workspace request per session generation, holding
    // the canonical digest, the one run it reserved and that run's recorded
    // outcome. It has no foreign key on purpose: a pruned run must not take
    // the recorded answer with it, because replaying the key must never admit
    // a second run.
    (
        "workspace_admissions",
        "CREATE TABLE IF NOT EXISTS workspace_admissions (
  session_id TEXT NOT NULL,
  session_generation INTEGER NOT NULL,
  session_owner TEXT,
  request_id TEXT NOT NULL,
  digest BLOB NOT NULL,
  run_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('reserved','admitted','closed')),
  outcome TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (session_id, session_generation, request_id)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS workspace_admissions_run_identity
  ON workspace_admissions(run_id)",
    ),
    // A client's lifetime on a run, separate from environment resource holds
    // (`attachments`) and from remote workload custody. Rows end with the run.
    (
        "run_attachments",
        "CREATE TABLE IF NOT EXISTS run_attachments (
  attachment_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  session_generation INTEGER NOT NULL,
  lifetime TEXT NOT NULL CHECK (lifetime IN ('owner','secondary')),
  state TEXT NOT NULL CHECK (state IN ('attached','released')),
  release TEXT CHECK (release IN ('eof','detach')),
  created_at INTEGER NOT NULL,
  released_at INTEGER
) STRICT;
CREATE INDEX IF NOT EXISTS run_attachments_run
  ON run_attachments(run_id, state)",
    ),
    // 052 §8.7: which run is an environment's workspace session. A Borrow
    // attachment alone cannot say it — a child run or a host `dev` hold can
    // borrow the same environment — so the session's own journal records the
    // mapping, and every read validates it against the run row and the
    // generation before it is used.
    (
        "workspace_sessions",
        "CREATE TABLE IF NOT EXISTS workspace_sessions (
  environment TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  session_generation INTEGER NOT NULL,
  environment_generation INTEGER NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;",
    ),
    // 052 §8.7/§8.11: the successor environment generation a workspace
    // activation reserved, plus the two runs it binds — the environment
    // activation run admitted at E and the Task-owned session staged against
    // it at its own generation S. One row per (environment, generation): a
    // later generation is a new activation, and the earlier row is kept as the
    // evidence a recovery reads. `state` is text without a CHECK on purpose, so
    // a reader that meets a state it does not know refuses instead of guessing.
    // No foreign key: a pruned run must not take its reservation with it.
    (
        "workspace_activation_reservations",
        "CREATE TABLE IF NOT EXISTS workspace_activation_reservations (
  environment TEXT NOT NULL,
  environment_generation INTEGER NOT NULL,
  reservation_id TEXT NOT NULL UNIQUE,
  state TEXT NOT NULL,
  environment_run TEXT,
  session_owner TEXT,
  session_run TEXT,
  session_generation INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (environment, environment_generation)
) STRICT;
CREATE INDEX IF NOT EXISTS workspace_activation_reservations_session
  ON workspace_activation_reservations(session_run)",
    ),
    (
        "workspace_session_runs",
        "CREATE TABLE IF NOT EXISTS workspace_session_runs (
  run_id TEXT PRIMARY KEY,
  owner TEXT NOT NULL,
  generation INTEGER NOT NULL,
  reservation_id TEXT,
  base_generation INTEGER NOT NULL,
  gate_key TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS workspace_session_runs_owner ON workspace_session_runs(owner)",
    ),
];

/// Indexes §7.4 retention needs, as additive schema-1 changes. Retention's
/// candidates and deletes are index-driven, so a sweep costs the same on a
/// small store and a hundred-thousand-run store.
const ADDITIVE_INDEXES: [(&str, &str); 11] = [
    (
        "runs_settled",
        "CREATE INDEX IF NOT EXISTS runs_settled ON runs(created_at)",
    ),
    (
        "resources_updated",
        "CREATE INDEX IF NOT EXISTS resources_updated ON resources(updated_at)",
    ),
    (
        "claims_holder",
        "CREATE INDEX IF NOT EXISTS claims_holder ON claims(holder)",
    ),
    (
        "claims_run",
        "CREATE INDEX IF NOT EXISTS claims_run ON claims(run_id)",
    ),
    (
        "runs_parent",
        "CREATE INDEX IF NOT EXISTS runs_parent ON runs(parent_run)",
    ),
    (
        "requests_run",
        "CREATE INDEX IF NOT EXISTS requests_run ON requests(run_id)",
    ),
    (
        "attachments_run",
        "CREATE INDEX IF NOT EXISTS attachments_run ON attachments(run_id)",
    ),
    // 052 §8.11: one run identity belongs to at most one keyed request, so a
    // run can never be attributed to another session's key. The admission
    // transaction refuses a collision by name; this index holds the same line
    // for any writer that reaches the table another way. The earlier draft
    // carried a non-unique index under the old name.
    (
        "workspace_admissions_run_identity",
        "DROP INDEX IF EXISTS workspace_admissions_run; \
         CREATE UNIQUE INDEX IF NOT EXISTS workspace_admissions_run_identity \
         ON workspace_admissions(run_id)",
    ),
    (
        "object_refs_resource",
        "CREATE INDEX IF NOT EXISTS object_refs_resource ON object_refs(resource)",
    ),
    (
        "checkouts_live_seen",
        "CREATE INDEX IF NOT EXISTS checkouts_live_seen ON checkouts(state, last_seen)",
    ),
    (
        "checkouts_tombstoned",
        "CREATE INDEX IF NOT EXISTS checkouts_tombstoned          ON checkouts(state, unreferenced_since)",
    ),
];

/// Schema 1, the tables of §7.2. Run, event, attachment, claim and request
/// rows are written by their owning primitives; the store owns the shape.
const SCHEMA_V1: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE checkouts (
  token TEXT PRIMARY KEY,
  kind INTEGER NOT NULL,
  path_hint TEXT,
  git_dir_hint TEXT,
  state TEXT NOT NULL CHECK (state IN ('live','tombstoned')),
  first_seen INTEGER NOT NULL,
  last_seen INTEGER NOT NULL,
  tombstoned_at INTEGER,
  unreferenced_since INTEGER
) STRICT;
CREATE TABLE resources (
  name TEXT PRIMARY KEY,
  canonical BLOB NOT NULL UNIQUE,
  kind INTEGER NOT NULL,
  scope TEXT NOT NULL,
  checkout TEXT REFERENCES checkouts(token),
  label TEXT NOT NULL,
  authority TEXT NOT NULL,
  generation INTEGER NOT NULL CHECK (generation >= 0),
  desired TEXT,
  last_applied TEXT,
  backend TEXT,
  state TEXT,
  engine TEXT,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX resources_checkout ON resources(checkout);
CREATE TABLE runs (
  run_id TEXT PRIMARY KEY,
  owner TEXT NOT NULL REFERENCES resources(name),
  generation INTEGER NOT NULL,
  kind TEXT NOT NULL,
  placement TEXT NOT NULL,
  phase TEXT NOT NULL CHECK (phase IN ('admitted','starting','active','stopping','closed')),
  workload_outcome TEXT,
  cleanup_outcome TEXT CHECK (cleanup_outcome IN ('complete','incomplete')),
  invocation TEXT,
  digest BLOB,
  owner_identity TEXT,
  supervisor_identity TEXT,
  sentinel_identity TEXT,
  parent_run TEXT REFERENCES runs(run_id) ON DELETE SET NULL,
  stop_cause TEXT,
  residue_ack TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  closed_at INTEGER
) STRICT;
CREATE INDEX runs_owner ON runs(owner, created_at);
CREATE INDEX runs_settled ON runs(created_at);

CREATE TABLE events (
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
  seq INTEGER NOT NULL,
  effect_id TEXT,
  kind TEXT NOT NULL,
  target TEXT,
  settlement TEXT,
  state TEXT NOT NULL,
  receipt TEXT,
  error TEXT,
  deadline_boot TEXT,
  deadline_nanos INTEGER,
  wall_time INTEGER NOT NULL,
  PRIMARY KEY (run_id, seq)
) STRICT;
CREATE TABLE attachments (
  environment TEXT NOT NULL REFERENCES resources(name),
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
  env_generation INTEGER NOT NULL,
  mode TEXT NOT NULL,
  state TEXT NOT NULL,
  PRIMARY KEY (environment, run_id)
) STRICT;
CREATE TABLE claims (
  resource TEXT PRIMARY KEY REFERENCES resources(name),
  holder TEXT REFERENCES resources(name) ON DELETE SET NULL,
  run_id TEXT REFERENCES runs(run_id) ON DELETE SET NULL,
  reservation_token TEXT,
  state TEXT NOT NULL CHECK (state IN ('reserved','live','quarantined','released')),
  generation INTEGER NOT NULL,
  revision INTEGER NOT NULL,
  quarantine_boot TEXT,
  quarantine_nanos INTEGER,
  details TEXT
) STRICT;
CREATE TABLE requests (
  request_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
  expected_generation INTEGER NOT NULL,
  action TEXT NOT NULL,
  cause TEXT,
  requester TEXT,
  state TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  resolved_at INTEGER
) STRICT;
CREATE TABLE objects (
  digest TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  path TEXT NOT NULL,
  size INTEGER NOT NULL,
  created_at INTEGER NOT NULL
) STRICT;
CREATE TABLE object_refs (
  digest TEXT NOT NULL REFERENCES objects(digest) ON DELETE CASCADE,
  resource TEXT NOT NULL REFERENCES resources(name),
  role TEXT NOT NULL CHECK (role IN ('desired','last_applied','superseded')),
  seq INTEGER NOT NULL,
  PRIMARY KEY (digest, resource, role)
) STRICT;
CREATE TABLE foreground_terminals (
  run_id TEXT PRIMARY KEY,
  checkout TEXT NOT NULL,
  boot TEXT NOT NULL,
  task TEXT NOT NULL,
  outcome TEXT NOT NULL,
  exit_code INTEGER,
  signal INTEGER,
  cleanup TEXT NOT NULL,
  workload_groups INTEGER NOT NULL,
  closed_at INTEGER NOT NULL
) STRICT;
CREATE INDEX foreground_terminals_closed ON foreground_terminals(closed_at);
CREATE TABLE workspace_admissions (
  session_id TEXT NOT NULL,
  session_generation INTEGER NOT NULL,
  session_owner TEXT,
  request_id TEXT NOT NULL,
  digest BLOB NOT NULL,
  run_id TEXT NOT NULL,
  state TEXT NOT NULL CHECK (state IN ('reserved','admitted','closed')),
  outcome TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  PRIMARY KEY (session_id, session_generation, request_id)
) STRICT;
CREATE TABLE run_attachments (
  attachment_id TEXT PRIMARY KEY,
  run_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  session_generation INTEGER NOT NULL,
  lifetime TEXT NOT NULL CHECK (lifetime IN ('owner','secondary')),
  state TEXT NOT NULL CHECK (state IN ('attached','released')),
  release TEXT CHECK (release IN ('eof','detach')),
  created_at INTEGER NOT NULL,
  released_at INTEGER
) STRICT;
CREATE TABLE logs (
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
  stream TEXT NOT NULL,
  path TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  truncated INTEGER NOT NULL,
  redaction TEXT NOT NULL,
  PRIMARY KEY (run_id, stream)
) STRICT;
CREATE INDEX resources_updated ON resources(updated_at);
CREATE INDEX claims_holder ON claims(holder);
CREATE INDEX claims_run ON claims(run_id);
CREATE INDEX runs_parent ON runs(parent_run);
CREATE INDEX requests_run ON requests(run_id);
CREATE INDEX attachments_run ON attachments(run_id);
CREATE UNIQUE INDEX workspace_admissions_run_identity ON workspace_admissions(run_id);
CREATE INDEX run_attachments_run ON run_attachments(run_id, state);
CREATE INDEX object_refs_resource ON object_refs(resource);
CREATE INDEX checkouts_live_seen ON checkouts(state, last_seen);
CREATE INDEX checkouts_tombstoned ON checkouts(state, unreferenced_since);
";
