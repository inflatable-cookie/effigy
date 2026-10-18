//! Cooperative lazy refresh over committed per-file transactions.
//! A reader may see different file generations and reports `refreshing (n/N)`
//! while another process holds the refresh lock. No database copy is made.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::CodeGraphError;
use crate::index::{
    graph_freshness_payload, run_index_unlocked_in_scope, stale_index_freshness_payload,
    stale_paths_in_scope, IndexReport,
};
use crate::json::{GraphFreshnessPayload, GraphLockPayload};
use crate::phase::{self, GraphPhase};
use crate::scope::GraphScope;
use crate::storage::GraphStore;

/// How long a query waits for an in-flight refresh before serving current
/// data that is still marked stale.
const IN_FLIGHT_WAIT_MS: u64 = 2_500;
const EXPLICIT_REFRESH_WAIT_MS: u64 = 10_000;
const LOCK_POLL_MS: u64 = 100;
/// Extra polls allowed after a recorded holder pid is dead before treating
/// the lock as abandoned for lookup purposes.
const STALE_HOLDER_WAIT_MS: u64 = 200;

/// Cross-process exclusive lock guarding graph re-indexing of one scope.
///
/// Acquired by lazy refresh, `graph watch` batches, and explicit `graph index`
/// runs, so parallel processes never race to rebuild the same scope. An
/// independent catalog locks its own database directory; shared scopes lock
/// the root graph directory.
pub(crate) struct RefreshLock {
    file: File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockIdentityRecord {
    pid: u32,
    acquired_unix_ms: u64,
}

/// Observed refresh-lock identity for diagnostics and lookup decisions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshLockInspection {
    pub held: bool,
    pub identity: Option<LockIdentityRecordView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockIdentityRecordView {
    pub pid: u32,
    pub acquired_unix_ms: u64,
    pub age_ms: u64,
    pub stale_holder: bool,
}

impl RefreshLockInspection {
    pub fn payload(&self) -> GraphLockPayload {
        GraphLockPayload {
            held: self.held,
            pid: self.identity.as_ref().map(|identity| identity.pid),
            age_ms: self.identity.as_ref().map(|identity| identity.age_ms),
            stale_holder: self
                .identity
                .as_ref()
                .is_some_and(|identity| identity.stale_holder),
        }
    }
}

impl RefreshLock {
    /// Acquire the refresh lock immediately, or return `None` when another
    /// process currently holds it.
    pub(crate) fn try_acquire(scope: &GraphScope) -> Result<Option<Self>, CodeGraphError> {
        let mut file = open_lock_file(scope)?;
        match file.try_lock_exclusive() {
            Ok(()) => {
                write_lock_identity(&mut file)?;
                Ok(Some(Self { file }))
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Acquire the refresh lock, polling up to `wait_ms` for an in-flight
    /// refresh to release it. Returns `None` when the wait expires.
    ///
    /// A recorded holder whose pid is dead shortens the wait: the kernel
    /// releases flock when the process exits, so a stale pid is not a reason
    /// to spend the full in-flight budget.
    pub(crate) fn acquire_wait(
        scope: &GraphScope,
        wait_ms: u64,
    ) -> Result<Option<Self>, CodeGraphError> {
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        let stale_deadline =
            Instant::now() + Duration::from_millis(STALE_HOLDER_WAIT_MS.min(wait_ms));
        let mut waited = false;
        loop {
            crate::deadline::check()?;
            if let Some(lock) = Self::try_acquire(scope)? {
                return Ok(Some(lock));
            }
            let inspection = inspect_refresh_lock(&scope.paths().refresh_lock_path);
            let stale_holder = inspection
                .identity
                .as_ref()
                .is_some_and(|identity| identity.stale_holder);
            let now = Instant::now();
            if now >= deadline || (stale_holder && now >= stale_deadline) {
                return Ok(None);
            }
            if !waited {
                // Only a real wait is worth reporting: an uncontended acquire
                // must not overwrite the phase the caller is actually in.
                phase::enter(GraphPhase::RefreshLockWait);
                waited = true;
            }
            std::thread::sleep(Duration::from_millis(LOCK_POLL_MS));
        }
    }
}

impl Drop for RefreshLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

#[cfg(test)]
impl RefreshLock {
    /// Rewrite lock metadata with a pid that is not this process, so a waiter
    /// can observe a stale holder while this handle still owns the flock.
    pub(crate) fn plant_stale_identity(&mut self, pid: u32) -> Result<(), CodeGraphError> {
        let record = LockIdentityRecord {
            pid,
            acquired_unix_ms: unix_now_ms().saturating_sub(60_000),
        };
        let encoded = serde_json::to_vec(&record)?;
        self.file.set_len(0)?;
        self.file.seek(SeekFrom::Start(0))?;
        self.file.write_all(&encoded)?;
        self.file.write_all(b"\n")?;
        self.file.flush()?;
        Ok(())
    }
}

fn open_lock_file(scope: &GraphScope) -> Result<File, CodeGraphError> {
    let path = scope.paths().refresh_lock_path;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(path)?)
}

fn write_lock_identity(file: &mut File) -> Result<(), CodeGraphError> {
    let record = LockIdentityRecord {
        pid: std::process::id(),
        acquired_unix_ms: unix_now_ms(),
    };
    let encoded = serde_json::to_vec(&record)?;
    file.set_len(0)?;
    file.seek(SeekFrom::Start(0))?;
    file.write_all(&encoded)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn unix_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn process_is_alive(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    #[cfg(unix)]
    {
        extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        // SAFETY: `kill(pid, 0)` is the POSIX existence check and does not
        // deliver a signal. ESRCH (3) means the process table has no such pid.
        let rc = unsafe { kill(pid as i32, 0) };
        if rc == 0 {
            return true;
        }
        std::io::Error::last_os_error().raw_os_error() != Some(3)
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Inspect a refresh lock without taking it or creating the file.
pub fn inspect_refresh_lock(lock_path: &Path) -> RefreshLockInspection {
    let held = refresh_lock_held(lock_path);
    let identity = read_lock_identity(lock_path).map(|record| {
        let age_ms = unix_now_ms().saturating_sub(record.acquired_unix_ms);
        let stale_holder = !process_is_alive(record.pid);
        LockIdentityRecordView {
            pid: record.pid,
            acquired_unix_ms: record.acquired_unix_ms,
            age_ms,
            stale_holder,
        }
    });
    RefreshLockInspection { held, identity }
}

fn refresh_lock_held(lock_path: &Path) -> bool {
    let Ok(file) = OpenOptions::new().read(true).write(true).open(lock_path) else {
        return false;
    };
    match file.try_lock_exclusive() {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(error) => error.kind() == std::io::ErrorKind::WouldBlock,
    }
}

fn read_lock_identity(lock_path: &Path) -> Option<LockIdentityRecord> {
    let mut file = OpenOptions::new().read(true).open(lock_path).ok()?;
    let mut buf = String::new();
    file.read_to_string(&mut buf).ok()?;
    serde_json::from_str(buf.trim()).ok()
}

/// Outcome of a lazy freshness pass on a query.
pub struct RefreshOutcome {
    /// Freshness payload describing current graph trust.
    pub freshness: GraphFreshnessPayload,
    /// Human-readable notes describing what the pass did (empty when fresh).
    pub notes: Vec<String>,
    /// Which database a lookup should open after this pass.
    pub source: RefreshSource,
}

/// Which corpus a lookup should read after a freshness pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshSource {
    /// The live scope database, current or honestly marked stale.
    Live,
    /// No committed files are available. Callers must return status only.
    Unavailable,
}

/// How a lookup should treat refresh and cold builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshPolicy {
    /// Skip refresh and lock wait; read the committed files.
    pub stale_index: bool,
    /// When no committed index exists, build one under the refresh lock.
    pub build_missing: bool,
}

impl RefreshPolicy {
    /// Crate and scan default: refresh stale, build missing.
    pub fn query() -> Self {
        Self {
            stale_index: false,
            build_missing: true,
        }
    }

    /// Bounded CLI lookup: `--stale-index` reads the committed files.
    pub fn lookup(stale_index: bool) -> Self {
        Self {
            stale_index,
            build_missing: !stale_index,
        }
    }
}

pub(crate) fn run_index_exclusive(scope: &GraphScope) -> Result<IndexReport, CodeGraphError> {
    run_index_exclusive_with_wait(scope, EXPLICIT_REFRESH_WAIT_MS)
}

pub(crate) fn run_index_exclusive_with_wait(
    scope: &GraphScope,
    wait_ms: u64,
) -> Result<IndexReport, CodeGraphError> {
    let Some(_lock) = RefreshLock::acquire_wait(scope, wait_ms)? else {
        let inspection = inspect_refresh_lock(&scope.paths().refresh_lock_path);
        return Err(CodeGraphError::validation(lock_busy_message(
            scope,
            wait_ms,
            &inspection,
        )));
    };
    run_index_unlocked_in_scope(scope)
}

fn lock_busy_message(
    scope: &GraphScope,
    wait_ms: u64,
    inspection: &RefreshLockInspection,
) -> String {
    let catalog = format!("catalog `{}`", scope.alias());
    match inspection.identity.as_ref() {
        Some(identity) if identity.stale_holder => format!(
            "graph refresh lock remained busy for {wait_ms}ms ({catalog}; stale holder pid {} age {}ms); run `effigy graph index --json` once the abandoned lock clears",
            identity.pid, identity.age_ms
        ),
        Some(identity) => format!(
            "graph refresh lock remained busy for {wait_ms}ms ({catalog}; holder pid {} age {}ms); wait for that process or pass `--stale-index` to read the committed files",
            identity.pid, identity.age_ms
        ),
        None => format!(
            "graph refresh lock remained busy for {wait_ms}ms ({catalog}); wait for the holder or pass `--stale-index` to read the committed files"
        ),
    }
}

/// Ensure one scope is current, rebuilding it on demand.
///
/// Fresh scopes cost one scoped walk. Stale or missing scopes are rebuilt
/// incrementally under the scope's refresh lock; when another process is
/// already refreshing, the call waits a bounded budget and then reports the
/// true trust state instead of inventing one.
pub fn ensure_fresh(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<RefreshOutcome, CodeGraphError> {
    ensure_fresh_with_progress(scope, store, |_| {})
}

/// Same freshness pass with a progress callback that receives the refresh
/// verdict before any rebuild walk starts, so a caller can announce cold or
/// stale work while it is still inside the caller's bound. The verdict is
/// derived from the same single freshness scan that feeds the rebuild — no
/// duplicate scan is performed.
pub(crate) fn ensure_fresh_with_progress(
    scope: &GraphScope,
    store: &GraphStore,
    progress: impl FnMut(RefreshPending),
) -> Result<RefreshOutcome, CodeGraphError> {
    ensure_fresh_with_wait_and_progress(
        scope,
        store,
        IN_FLIGHT_WAIT_MS,
        RefreshPolicy::query(),
        progress,
    )
}

pub(crate) fn ensure_fresh_with_wait_and_progress(
    scope: &GraphScope,
    store: &GraphStore,
    in_flight_wait_ms: u64,
    policy: RefreshPolicy,
    mut progress: impl FnMut(RefreshPending),
) -> Result<RefreshOutcome, CodeGraphError> {
    crate::deadline::check()?;
    let inspection = inspect_refresh_lock(&scope.paths().refresh_lock_path);
    if inspection.held {
        return committed_outcome(scope, store, &inspection, &[]);
    }
    if policy.stale_index {
        return committed_outcome(scope, store, &inspection, &[]);
    }
    let ready = store.counts_in_scope(scope)?.files > 0;
    if ready && crate::git::git_gate_says_fresh(scope, store)? {
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(
                true,
                true,
                &[],
                store.failed_diagnostic_paths_in_scope(scope)?.len(),
            ),
            notes: Vec::new(),
            source: RefreshSource::Live,
        });
    }
    let stale = stale_paths_in_scope(scope, store)?;
    if ready && stale.is_empty() {
        return current_outcome(scope, store, Vec::new());
    }
    progress(if ready {
        RefreshPending::Stale
    } else {
        RefreshPending::Cold
    });
    if !ready && !policy.build_missing {
        return committed_outcome(scope, store, &inspection, &stale);
    }
    let Some(lock) = RefreshLock::acquire_wait(scope, in_flight_wait_ms)? else {
        return committed_outcome(
            scope,
            store,
            &inspect_refresh_lock(&scope.paths().refresh_lock_path),
            &stale,
        );
    };
    let started = Instant::now();
    let report = run_index_unlocked_in_scope(scope)?;
    drop(lock);
    current_outcome(
        scope,
        store,
        vec![format!(
            "{} ({} files in {}ms)",
            if ready {
                "graph auto-refreshed"
            } else {
                "graph index built on demand"
            },
            report.indexed_files,
            started.elapsed().as_millis()
        )],
    )
}

fn current_outcome(
    scope: &GraphScope,
    store: &GraphStore,
    notes: Vec<String>,
) -> Result<RefreshOutcome, CodeGraphError> {
    let stale = stale_paths_in_scope(scope, store)?;
    Ok(RefreshOutcome {
        freshness: graph_freshness_payload(
            store.counts_in_scope(scope)?.files > 0,
            true,
            &stale,
            store.failed_diagnostic_paths_in_scope(scope)?.len(),
        ),
        notes,
        source: RefreshSource::Live,
    })
}

fn committed_outcome(
    scope: &GraphScope,
    store: &GraphStore,
    inspection: &RefreshLockInspection,
    stale: &[String],
) -> Result<RefreshOutcome, CodeGraphError> {
    let ready = store.counts_in_scope(scope)?.files > 0;
    let failed = store.failed_diagnostic_paths_in_scope(scope)?.len();
    let mut freshness = if ready {
        stale_index_freshness_payload(stale, failed, inspection.held.then(|| inspection.payload()))
    } else {
        graph_freshness_payload(false, true, stale, failed)
    };
    if inspection.held {
        freshness.state = "refreshing".to_owned();
        freshness.stale = true;
        freshness.lock = Some(inspection.payload());
        let progress = store
            .metadata_value("refresh_progress")?
            .unwrap_or_else(|| "0/?".to_owned());
        freshness.summary = format!(
            "graph refreshing ({progress}); serving committed files; {}",
            lock_busy_message(scope, 0, inspection)
        );
    }
    Ok(RefreshOutcome {
        freshness,
        notes: Vec::new(),
        source: if ready {
            RefreshSource::Live
        } else {
            RefreshSource::Unavailable
        },
    })
}

/// Open one SQLite read transaction over committed data after the lazy refresh.
/// The transaction fixes the view across a query's multiple SQL reads.
pub fn open_query_store(
    scope: &GraphScope,
    policy: RefreshPolicy,
) -> Result<(Option<GraphStore>, GraphFreshnessPayload), CodeGraphError> {
    let existing = GraphStore::open_read_only(scope)?;
    if existing.is_none() && policy.stale_index {
        let mut freshness = graph_freshness_payload(false, false, &[], 0);
        let lock = inspect_refresh_lock(&scope.paths().refresh_lock_path);
        if lock.held {
            freshness.state = "refreshing".to_owned();
            freshness.stale = true;
            freshness.lock = Some(lock.payload());
            freshness.summary =
                "graph refreshing (0/?); no committed files yet; retry or run `effigy graph index`"
                    .to_owned();
        }
        return Ok((None, freshness));
    }
    let store = match existing {
        Some(store) => store,
        None => GraphStore::open_for_scope(scope)?,
    };
    let outcome =
        ensure_fresh_with_wait_and_progress(scope, &store, IN_FLIGHT_WAIT_MS, policy, |_| {})?;
    let freshness = apply_notes(outcome.freshness, &outcome.notes);
    drop(store);
    if outcome.source == RefreshSource::Unavailable {
        return Ok((None, freshness));
    }
    let store = GraphStore::open_read_only(scope)?;
    if let Some(store) = store.as_ref() {
        store.begin_read()?;
    }
    Ok((store, freshness))
}

fn apply_notes(mut freshness: GraphFreshnessPayload, notes: &[String]) -> GraphFreshnessPayload {
    if !notes.is_empty() {
        freshness.summary = format!("{} ({})", freshness.summary, notes.join("; "));
    }
    freshness
}

/// The refresh verdict reported by the lazy-refresh progress callback. The
/// verdict is derived from the same freshness scan that feeds the rebuild, so
/// callers can announce cold or stale work without a second scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshPending {
    /// The next query verifies freshness without touching graph state.
    Current,
    /// The graph store has no indexed files; the next query builds the index.
    Cold,
    /// The index exists but is stale; the next query rebuilds changed parts.
    Stale,
}
