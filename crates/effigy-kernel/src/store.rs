//! The runtime store (§5.5, §7.2): one SQLite `kernel.db` under the state
//! root, typed records and multi-record transactions.
//!
//! SQLite opens `kernel.db` by path. The kernel first creates or verifies the
//! file descriptor-relative (regular, operator-owned, 0600, one link, bounded)
//! inside a verified 0700 root, and SQLite then opens it with
//! `SQLITE_OPEN_NOFOLLOW`, so no symlinked component is followed (K13).
use crate::ctx::{EffectClass, EffectContext, Interrupt, ObserveContext};
use crate::fs::{FileName, FsError, IoError, Trust, FILE_MODE};
use crate::gate::{self, Gate, GateError, OwnerState, RunPhaseFact, Wait};
use crate::id::{CheckoutId, Generation, IdentityError, ResourceId, RunId, Scope};
use crate::ident::BootInstant;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::root::StateRoot;
use rusqlite::{params, Connection, ErrorCode, OpenFlags, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const SCHEMA_VERSION: i64 = 1;
pub const DB_NAME: &str = "kernel.db";
/// Size bound for `kernel.db`; retention keeps it far below this.
pub const MAX_DB_BYTES: u64 = 1 << 30;
const PAGE_SIZE: u64 = 4096;
const BUILD: &str = concat!("effigy-kernel ", env!("CARGO_PKG_VERSION"));
const SQLITE_READONLY_ROLLBACK: i32 = 776;

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
    /// The busy timeout from the context elapsed.
    Busy,
    /// Mutate refuses; `Store::repair` is the explicit repair (moves it aside).
    Corrupt(String),
    /// A hot rollback journal; the next Mutate open recovers it.
    NeedsRecovery,
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
    WrongGuard,
    TooLarge {
        limit: u64,
    },
    NotCorrupt,
    /// A pointer commit must select a strictly newer generation.
    StaleGeneration {
        current: u64,
    },
    UnknownResource,
}
impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Corrupt(reason) => write!(
                f,
                "kernel.db is unreadable ({reason}); the explicit store repair moves it aside"
            ),
            other => write!(f, "store refused: {other:?}"),
        }
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
            ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => {
                Self::Corrupt(error.to_string())
            }
            ErrorCode::DatabaseBusy | ErrorCode::DatabaseLocked => Self::Busy,
            ErrorCode::DiskFull => io(libc::ENOSPC),
            ErrorCode::PermissionDenied | ErrorCode::CannotOpen | ErrorCode::ReadOnly
                if code.extended_code == SQLITE_READONLY_ROLLBACK =>
            {
                Self::NeedsRecovery
            }
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

/// A writable store handle. Hold one per operation; never across a
/// subprocess, prompt or network call (I-S2).
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}
impl Store {
    /// Opens for writing: creates `kernel.db` 0600, applies the settings in
    /// §5.5, and creates or checks the schema (I-S5).
    pub fn open(cx: &EffectContext, root: &StateRoot) -> Result<Self, StoreError> {
        let busy = busy_timeout(cx.operation().remaining())?;
        if !root.writable() {
            return Err(StoreError::Untrusted {
                path: root.path().into(),
                reason: "state root opened read-only".into(),
            });
        }
        let name = FileName::new(DB_NAME)?;
        let (_, stat) = root
            .dir()
            .open_file(&name, libc::O_RDWR, Some(FILE_MODE), Trust::Private)?
            .expect("created store file");
        if stat.st_size as u64 > MAX_DB_BYTES {
            return Err(StoreError::TooLarge {
                limit: MAX_DB_BYTES,
            });
        }
        check_journal(root)?;
        let conn = Connection::open_with_flags(
            root.dir().join(DB_NAME),
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(busy)?;
        let mode: String = conn.query_row("PRAGMA journal_mode=DELETE", [], |row| row.get(0))?;
        if mode != "delete" {
            return Err(StoreError::Corrupt(format!("journal mode {mode}")));
        }
        conn.execute_batch(&format!(
            "PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON; PRAGMA trusted_schema=OFF; \
             PRAGMA cell_size_check=ON; PRAGMA max_page_count={};",
            MAX_DB_BYTES / PAGE_SIZE
        ))?;
        let mut store = Self { conn };
        store.schema(cx)?;
        Ok(store)
    }
    /// Read-only open (R2): no file is created or written, and no lock other
    /// than SQLite's shared read lock is taken.
    pub fn read(cx: &ObserveContext, root: &StateRoot) -> Observed<StoreReader> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        let busy = cx.remaining();
        let name = FileName::new(DB_NAME).expect("fixed name");
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
        let opened = Connection::open_with_flags(
            root.dir().join(DB_NAME),
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_NOFOLLOW
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(StoreError::from)
        .and_then(|conn| {
            conn.busy_timeout(busy)?;
            conn.execute_batch("PRAGMA query_only=1; PRAGMA trusted_schema=OFF;")?;
            let version = schema_version(&conn)?;
            Ok(StoreReader { conn, version })
        });
        match opened {
            Ok(reader) => Observed::Verified(reader, evidence("kernel store")),
            Err(error) => Observed::Unknown(read_unknown(&error)),
        }
    }
    /// One `BEGIN IMMEDIATE` transaction. The closure's records commit
    /// together or not at all (I-S1). The busy wait comes from the context.
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
        cx.operation()
            .check("store transaction")
            .map_err(StoreError::Interrupted)?;
        self.conn
            .busy_timeout(busy_timeout(cx.operation().remaining())?)
            .map_err(StoreError::from)?;
        let flag = TxnFlag::enter()?;
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::from)?;
        let value = f(&mut Txn {
            conn: &tx,
            now: wall_ms(),
        })?;
        crate::fault_point!(StoreCommitBefore).map_err(StoreError::from)?;
        tx.commit().map_err(StoreError::from)?;
        drop(flag);
        crate::fault_point!(StoreCommitAfter).map_err(StoreError::from)?;
        Ok(value)
    }
    /// O6: integrity and foreign-key checks.
    pub fn check_integrity(&self) -> Result<(), StoreError> {
        let integrity: String = self
            .conn
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(StoreError::Corrupt(integrity));
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
        let gate = ResourceId::store(Scope::Machine, "kernel")?;
        let _guard = Gate::acquire(cx, root, &[gate], Wait::FailFast)?;
        match Self::open(cx, root) {
            Ok(_) => return Err(StoreError::NotCorrupt),
            Err(StoreError::Corrupt(_)) => {}
            Err(other) => return Err(other),
        }
        let stamp = wall_ms();
        let aside = FileName::new(&format!("{DB_NAME}.corrupt-{stamp}"))?;
        let journal = FileName::new(&format!("{DB_NAME}-journal"))?;
        if root.dir().entry(&journal)?.is_some() {
            root.dir()
                .rename(&journal, &aside.with_suffix("-journal")?)?;
        }
        root.dir().rename(&FileName::new(DB_NAME)?, &aside)?;
        root.dir().sync()?;
        Ok(aside)
    }
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
    fn schema(&mut self, cx: &EffectContext) -> Result<(), StoreError> {
        self.txn(cx, |txn| {
            let tables: i64 = txn.conn.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )?;
            if tables == 0 {
                txn.conn.execute_batch(SCHEMA_V1)?;
                txn.conn.execute(
                    "INSERT INTO meta(key, value) VALUES ('schema_version', ?1), \
                     ('created_by', ?2), ('last_written_by', ?2)",
                    params![SCHEMA_VERSION.to_string(), BUILD],
                )?;
                return Ok(());
            }
            let found = schema_version(txn.conn)?;
            if found != SCHEMA_VERSION {
                return Err(StoreError::UnknownSchema { found });
            }
            // Schema 1 additive column: old readers select explicit columns;
            // Observe reads an older store without migrating it.
            if !has_backend(txn.conn)? {
                txn.conn
                    .execute_batch("ALTER TABLE resources ADD COLUMN backend TEXT")?;
            }
            if !has_column(txn.conn, "engine")? {
                txn.conn
                    .execute_batch("ALTER TABLE resources ADD COLUMN engine TEXT")?;
            }
            txn.conn.execute(
                "UPDATE meta SET value=?1 WHERE key='last_written_by' AND value<>?1",
                params![BUILD],
            )?;
            Ok(())
        })
    }
}

/// Typed operations inside one transaction. Not `Send`: it cannot leave the
/// thread that opened it.
pub struct Txn<'a> {
    conn: &'a Connection,
    now: i64,
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
                let owner: Observed<(String, String)> = self.row(
                    "resources",
                    "SELECT r.scope, r.name FROM runs JOIN resources r ON r.name=runs.owner \
                     WHERE runs.run_id=?1",
                    [run.to_string()],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                );
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
fn busy_timeout(remaining: Option<Duration>) -> Result<Duration, StoreError> {
    match remaining {
        Some(left) if left.is_zero() => Err(StoreError::Interrupted(Interrupt::TimedOut {
            phase: "store busy wait".into(),
            elapsed: Duration::ZERO,
        })),
        Some(left) => Ok(left.min(Duration::from_millis(i32::MAX as u64))),
        None => Err(StoreError::Unbounded),
    }
}
fn check_journal(root: &StateRoot) -> Result<(), StoreError> {
    let journal = FileName::new(&format!("{DB_NAME}-journal"))?;
    if let Some(stat) = root.dir().entry(&journal)? {
        crate::fs::check(
            &stat,
            Trust::Private,
            crate::fs::Kind::File,
            &root.dir().join(journal.as_str()),
        )?;
    }
    Ok(())
}
fn schema_version(conn: &Connection) -> Result<i64, StoreError> {
    let value: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| match StoreError::from(error) {
            StoreError::Sqlite { message, .. } => StoreError::Corrupt(message),
            other => other,
        })?;
    value
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| StoreError::Corrupt("missing schema version".into()))
}
fn read_unknown(error: &StoreError) -> UnknownReason {
    match error {
        StoreError::NeedsRecovery => UnknownReason::StoreNeedsRecovery,
        StoreError::Busy => UnknownReason::TimedOut {
            phase: "store read".into(),
        },
        StoreError::Io(io) if io.is_permission() => UnknownReason::PermissionDenied,
        other => UnknownReason::StoreUnreadable(format!("{other:?}")),
    }
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
CREATE TABLE logs (
  run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE CASCADE,
  stream TEXT NOT NULL,
  path TEXT NOT NULL,
  bytes INTEGER NOT NULL,
  truncated INTEGER NOT NULL,
  redaction TEXT NOT NULL,
  PRIMARY KEY (run_id, stream)
) STRICT;
";
