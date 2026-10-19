//! Explicit graph refreshes own a kernel store gate. Queries only observe
//! committed per-file transactions and never create or refresh graph state.
use crate::error::CodeGraphError;
use crate::index::{
    graph_freshness_payload, run_index_unlocked_in_scope, stale_index_freshness_payload,
    stale_paths_in_scope, IndexReport,
};
use crate::json::{GraphFreshnessPayload, GraphLockPayload};
use crate::scope::GraphScope;
use crate::storage::GraphStore;
use effigy_kernel::{
    ctx::{CancelToken, Deadline, EffectClass, Limits, OperationContext, Placement},
    gate::{Gate, GateError, GateGuard, GateState, Wait},
    id::{ResourceId, Scope},
    observe::Observed,
    root::StateRoot,
};
use sha2::{Digest, Sha256};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const EXPLICIT_REFRESH_WAIT_MS: u64 = 10_000;

pub(crate) struct RefreshLock {
    _gate: GateGuard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshLockInspection {
    pub held: bool,
    pub unknown: bool,
    pub identity: Option<LockIdentityRecordView>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockIdentityRecordView {
    pub pid: u32,
}
impl RefreshLockInspection {
    pub fn payload(&self) -> GraphLockPayload {
        GraphLockPayload {
            held: self.held,
            pid: self.identity.as_ref().map(|identity| identity.pid),
            // The kernel gate diagnostic carries process identity, not age.
            age_ms: None,
            stale_holder: false,
        }
    }
}
fn operation(class: EffectClass, budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        CancelToken::default(),
        Limits {
            capture_tail: 0,
            log_rotate: 0,
            log_keep: 0,
            line_max: 0,
            queue_bytes: 0,
            probe_concurrency: 1,
        },
        class,
        if effigy_core::container_detection::process_is_inside_effigy_workspace_container() {
            Placement::Guest
        } else {
            Placement::Host
        },
    )
}
fn gate_coordinates(lock_path: &Path) -> Result<(PathBuf, ResourceId), CodeGraphError> {
    let repo = lock_path
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .ok_or_else(|| CodeGraphError::validation("invalid graph store path"))?
        .canonicalize()?;
    // A derived cache is keyed by its canonical location, not a managed-run
    // checkout token. Moving it gives it a different store; aliases share it.
    let key = hex::encode(Sha256::digest(repo.as_os_str().as_bytes()));
    let id = ResourceId::store(Scope::Machine, &format!("graph:{key}"))
        .map_err(|err| CodeGraphError::validation(err.to_string()))?;
    #[cfg(not(test))]
    let home = StateRoot::home_from_env()
        .ok_or_else(|| CodeGraphError::validation("graph indexing requires an absolute HOME"))?;
    // Every unit-test scope owns a fresh TempDir. Never write the operator's HOME.
    #[cfg(test)]
    let home = repo;
    Ok((home, id))
}
impl RefreshLock {
    #[cfg(test)]
    pub(crate) fn try_acquire(scope: &GraphScope) -> Result<Option<Self>, CodeGraphError> {
        Self::acquire_wait(scope, 0)
    }
    pub(crate) fn acquire_wait(
        scope: &GraphScope,
        wait_ms: u64,
    ) -> Result<Option<Self>, CodeGraphError> {
        crate::deadline::check()?;
        let budget = crate::deadline::remaining()
            .unwrap_or(Duration::from_millis(wait_ms))
            .min(Duration::from_millis(wait_ms));
        let op = operation(
            EffectClass::RefreshDerived,
            crate::deadline::remaining().unwrap_or(Duration::from_secs(10)),
        );
        let cx = op
            .effects()
            .map_err(|err| CodeGraphError::validation(format!("{err:?}")))?;
        let (home, id) = gate_coordinates(&scope.paths().refresh_lock_path)?;
        let root = StateRoot::effects(&cx, &home)
            .map_err(|err| CodeGraphError::validation(err.to_string()))?;
        let wait = if wait_ms == 0 {
            Wait::FailFast
        } else {
            Wait::Until(Instant::now() + budget)
        };
        match Gate::acquire(&cx, &root, &[id], wait) {
            Ok(gate) => Ok(Some(Self { _gate: gate })),
            Err(GateError::Busy { .. } | GateError::TimedOut { .. }) => Ok(None),
            Err(err) => Err(CodeGraphError::validation(err.to_string())),
        }
    }
}
/// Probe with Observe authority: missing state is free; unobservable state is
/// conservatively busy. The kernel takes only a momentary nonblocking shared lock.
pub fn inspect_refresh_lock(lock_path: &Path) -> RefreshLockInspection {
    let inspected = (|| {
        let (home, id) = gate_coordinates(lock_path).ok()?;
        let cx = operation(EffectClass::Observe, Duration::from_millis(100))
            .observe()
            .ok()?;
        match StateRoot::observe(&cx, &home) {
            Observed::Verified(root, _) => Some(Gate::probe(&cx, &root, &id)),
            Observed::Absent(_) => Some(Observed::Verified(
                GateState::Free,
                effigy_kernel::observe::Evidence::new("no graph gate state".into(), None).ok()?,
            )),
            Observed::Unknown(_) => None,
        }
    })();
    match inspected {
        Some(Observed::Verified(GateState::Free, _)) | Some(Observed::Absent(_)) => {
            RefreshLockInspection {
                held: false,
                unknown: false,
                identity: None,
            }
        }
        Some(Observed::Verified(GateState::Held(holder), _)) => {
            let identity = match *holder {
                Observed::Verified(diag, _) => Some(LockIdentityRecordView { pid: diag.pid }),
                _ => None,
            };
            RefreshLockInspection {
                held: true,
                unknown: false,
                identity,
            }
        }
        _ => RefreshLockInspection {
            held: false,
            unknown: true,
            identity: None,
        },
    }
}

#[derive(Debug)]
pub struct RefreshOutcome {
    pub freshness: GraphFreshnessPayload,
    pub notes: Vec<String>,
    pub source: RefreshSource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshSource {
    Live,
    Unavailable,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshPolicy {
    pub stale_index: bool,
}
impl RefreshPolicy {
    pub fn query() -> Self {
        Self { stale_index: false }
    }
    pub fn lookup(stale_index: bool) -> Self {
        Self { stale_index }
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
        return Err(CodeGraphError::validation(lock_busy_message(
            scope,
            wait_ms,
            &inspect_refresh_lock(&scope.paths().refresh_lock_path),
        )));
    };
    crate::deadline::check()?;
    run_index_unlocked_in_scope(scope)
}
fn lock_busy_message(
    _scope: &GraphScope,
    wait_ms: u64,
    inspection: &RefreshLockInspection,
) -> String {
    match inspection.identity.as_ref() {
        Some(identity) => format!("graph refresh lock remained busy for {wait_ms}ms (holder pid {}); wait for that process or read committed files",identity.pid),
        None => format!("graph refresh lock remained busy for {wait_ms}ms; wait for the holder or read committed files"),
    }
}

/// Observe freshness without rebuilding, even when the index is stale.
pub fn ensure_fresh(
    scope: &GraphScope,
    store: &GraphStore,
) -> Result<RefreshOutcome, CodeGraphError> {
    let inspection = inspect_refresh_lock(&scope.paths().refresh_lock_path);
    let stale = stale_paths_in_scope(scope, store)?;
    let mut outcome = committed_outcome(scope, store, &inspection, &stale)?;
    if !inspection.held {
        outcome.freshness = graph_freshness_payload(
            store.counts()?.files > 0,
            true,
            &stale,
            store.failed_diagnostic_paths()?.len(),
        );
    }
    if inspection.unknown {
        outcome.freshness.state = "refresh-recommended".into();
        outcome.freshness.stale = true;
        outcome.freshness.summary =
            "graph refresh gate is unobservable; run `effigy graph index` on the host".into();
    }
    Ok(outcome)
}
fn committed_outcome(
    scope: &GraphScope,
    store: &GraphStore,
    inspection: &RefreshLockInspection,
    stale: &[String],
) -> Result<RefreshOutcome, CodeGraphError> {
    let ready = store.counts()?.files > 0;
    let failed = store.failed_diagnostic_paths()?.len();
    let mut freshness = if ready {
        stale_index_freshness_payload(stale, failed, inspection.held.then(|| inspection.payload()))
    } else {
        graph_freshness_payload(false, true, stale, failed)
    };
    if inspection.unknown {
        freshness.state = "refresh-recommended".into();
        freshness.stale = true;
        freshness.summary =
            "graph refresh gate is unobservable; run `effigy graph index` on the host".into();
    } else if inspection.held {
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

/// One read-only transaction over committed data; no refresh or exclusive gate.
pub fn open_query_store(
    scope: &GraphScope,
    policy: RefreshPolicy,
) -> Result<(Option<GraphStore>, GraphFreshnessPayload), CodeGraphError> {
    let store = GraphStore::open_read_only(scope)?;
    let lock = inspect_refresh_lock(&scope.paths().refresh_lock_path);
    let Some(store) = store else {
        let mut freshness = graph_freshness_payload(false, scope.paths().db_path.is_file(), &[], 0);
        if GraphStore::has_wal(scope) {
            freshness.state = "refresh-recommended".into();
            freshness.stale = true;
            freshness.summary="graph stale (refresh interrupted or in progress); run `effigy graph index` to migrate WAL storage".into();
        } else if lock.held {
            freshness.state = "refreshing".into();
            freshness.stale = true;
            freshness.lock = Some(lock.payload());
            freshness.summary =
                "graph refreshing (0/?); no committed files yet; run `effigy graph index`".into();
        }
        return Ok((None, freshness));
    };
    store.begin_read()?;
    let outcome = if policy.stale_index {
        committed_outcome(scope, &store, &lock, &[])?
    } else {
        ensure_fresh(scope, &store)?
    };
    Ok((Some(store), outcome.freshness))
}
/// The refresh verdict reported by the lazy-refresh progress callback. The
/// verdict is derived from the same freshness scan that feeds the rebuild, so
/// callers can announce cold or stale work without a second scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshPending {
    /// The next query verifies freshness without touching graph state.
    Current,
    /// The graph store has no indexed files; explicit indexing is needed.
    Cold,
    /// The index exists but is stale; explicit indexing is needed.
    Stale,
}
