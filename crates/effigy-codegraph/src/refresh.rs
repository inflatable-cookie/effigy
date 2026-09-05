//! Lazy on-query graph refresh with a cross-process refresh lock.
//!
//! Graph data queries detect staleness, then rebuild the index on the fly
//! instead of returning stale results. On git repos whose index stamp matches
//! the current HEAD and whose working tree is clean, freshness is verified
//! without a full scan (a `git status` fast path); non-git repos and every git
//! failure mode fall back to the per-file scan-state walk. A cross-process
//! lock (`.effigy/graph/refresh.lock`) guarantees only one process re-indexes
//! at a time: concurrent queries wait a short budget for an in-flight refresh,
//! then serve whatever the lock holder left behind — with trust state that
//! says exactly how fresh it is.
//!
//! `graph status` intentionally stays report-only: it is the diagnostic
//! surface agents use to decide whether to index, so it must never mutate
//! graph state behind the caller's back.

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, Instant};

use fs2::FileExt;

use crate::error::CodeGraphError;
use crate::index::{
    graph_freshness_payload, run_index_unlocked, stale_paths_for_repo, IndexReport,
};
use crate::json::GraphFreshnessPayload;
use crate::phase::{self, GraphPhase};
use crate::storage::GraphStore;

/// How long a query waits for an in-flight refresh before serving current
/// data that is still marked stale.
const IN_FLIGHT_WAIT_MS: u64 = 2_500;
const EXPLICIT_REFRESH_WAIT_MS: u64 = 10_000;
const LOCK_POLL_MS: u64 = 100;

/// Cross-process exclusive lock guarding graph re-indexing.
///
/// Acquired by lazy refresh, `graph watch` batches, and explicit `graph index`
/// runs, so parallel processes never race to rebuild the same graph.
pub(crate) struct RefreshLock {
    file: File,
}

impl RefreshLock {
    /// Acquire the refresh lock immediately, or return `None` when another
    /// process currently holds it.
    pub(crate) fn try_acquire(repo_root: &Path) -> Result<Option<Self>, CodeGraphError> {
        let file = open_lock_file(repo_root)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { file })),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Acquire the refresh lock, polling up to `wait_ms` for an in-flight
    /// refresh to release it. Returns `None` when the wait expires.
    pub(crate) fn acquire_wait(
        repo_root: &Path,
        wait_ms: u64,
    ) -> Result<Option<Self>, CodeGraphError> {
        let deadline = Instant::now() + Duration::from_millis(wait_ms);
        let mut waited = false;
        loop {
            if let Some(lock) = Self::try_acquire(repo_root)? {
                return Ok(Some(lock));
            }
            if Instant::now() >= deadline {
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

fn open_lock_file(repo_root: &Path) -> Result<File, CodeGraphError> {
    let path = crate::paths::GraphPaths::for_repo(repo_root).refresh_lock_path;
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

/// Outcome of a lazy freshness pass on a query.
pub struct RefreshOutcome {
    /// Freshness payload describing current graph trust.
    pub freshness: GraphFreshnessPayload,
    /// Human-readable notes describing what the pass did (empty when fresh).
    pub notes: Vec<String>,
}

pub(crate) fn run_index_exclusive(repo_root: &Path) -> Result<IndexReport, CodeGraphError> {
    run_index_exclusive_with_wait(repo_root, EXPLICIT_REFRESH_WAIT_MS)
}

pub(crate) fn run_index_exclusive_with_wait(
    repo_root: &Path,
    wait_ms: u64,
) -> Result<IndexReport, CodeGraphError> {
    let Some(_lock) = RefreshLock::acquire_wait(repo_root, wait_ms)? else {
        return Err(CodeGraphError::validation(format!(
            "graph refresh lock remained busy for {wait_ms}ms"
        )));
    };
    run_index_unlocked(repo_root)
}

/// Ensure the graph is current for a query, rebuilding it on demand.
///
/// Fresh indexes cost one repo walk (the same staleness check every query
/// already paid). Stale or missing indexes are rebuilt incrementally under the
/// cross-process refresh lock; when another process is already refreshing, the
/// call waits a bounded budget and then reports the true trust state instead
/// of inventing one.
pub fn ensure_fresh(
    repo_root: &Path,
    store: &GraphStore,
) -> Result<RefreshOutcome, CodeGraphError> {
    ensure_fresh_with_progress(repo_root, store, |_| {})
}

/// Same freshness pass with a progress callback that receives the refresh
/// verdict before any rebuild walk starts, so a caller can announce cold or
/// stale work while it is still inside the caller's bound. The verdict is
/// derived from the same single freshness scan that feeds the rebuild — no
/// duplicate scan is performed.
pub(crate) fn ensure_fresh_with_progress(
    repo_root: &Path,
    store: &GraphStore,
    progress: impl FnMut(RefreshPending),
) -> Result<RefreshOutcome, CodeGraphError> {
    ensure_fresh_with_wait_and_progress(repo_root, store, IN_FLIGHT_WAIT_MS, progress)
}

pub(crate) fn ensure_fresh_with_wait_and_progress(
    repo_root: &Path,
    store: &GraphStore,
    in_flight_wait_ms: u64,
    mut progress: impl FnMut(RefreshPending),
) -> Result<RefreshOutcome, CodeGraphError> {
    phase::enter(GraphPhase::FreshnessScan);
    let counts = store.counts()?;
    if counts.files == 0 {
        progress(RefreshPending::Cold);
        return build_missing_index(repo_root, store, in_flight_wait_ms);
    }

    // Git skip-gate: when the index stamp matches the current HEAD and the
    // working tree is clean, the indexed tree provably equals the current
    // tree, so the freshness walk can be skipped. Non-git repos and any git
    // failure fall through to the scan-state walk unchanged.
    if crate::git::git_gate_says_fresh(repo_root, store)? {
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(
                true,
                true,
                &[],
                store.failed_diagnostic_paths()?.len(),
            ),
            notes: Vec::new(),
        });
    }

    let stale_paths = stale_paths_for_repo(repo_root, store)?;
    if stale_paths.is_empty() {
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(
                true,
                true,
                &[],
                store.failed_diagnostic_paths()?.len(),
            ),
            notes: Vec::new(),
        });
    }
    // The scan above is the single freshness scan for this pass: its result
    // feeds both the verdict and the rebuild below.
    progress(RefreshPending::Stale);

    let Some(lock) = RefreshLock::acquire_wait(repo_root, in_flight_wait_ms)? else {
        // Another process is refreshing. Check whether it finished inside the
        // wait window; report the honest trust state either way.
        let post_wait_stale = stale_paths_for_repo(repo_root, store)?;
        if post_wait_stale.is_empty() {
            return Ok(RefreshOutcome {
                freshness: graph_freshness_payload(
                    true,
                    true,
                    &[],
                    store.failed_diagnostic_paths()?.len(),
                ),
                notes: vec!["graph index refreshed by a concurrent process".to_owned()],
            });
        }
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(
                true,
                true,
                &post_wait_stale,
                store.failed_diagnostic_paths()?.len(),
            ),
            notes: vec![
                "graph refresh in progress by another process; results may be stale".to_owned(),
            ],
        });
    };

    // A concurrent refresh may have finished while we waited for the lock.
    let stale_after_wait = stale_paths_for_repo(repo_root, store)?;
    if stale_after_wait.is_empty() {
        drop(lock);
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(
                true,
                true,
                &[],
                store.failed_diagnostic_paths()?.len(),
            ),
            notes: vec!["graph index refreshed by a concurrent process".to_owned()],
        });
    }

    let started = Instant::now();
    let report = run_index_unlocked(repo_root)?;
    let duration_ms = started.elapsed().as_millis();
    let refreshed_files =
        report.new_paths.len() + report.changed_paths.len() + report.deleted_paths.len();
    let stale_after_refresh = stale_paths_for_repo(repo_root, store)?;
    drop(lock);

    Ok(RefreshOutcome {
        freshness: graph_freshness_payload(
            true,
            true,
            &stale_after_refresh,
            store.failed_diagnostic_paths()?.len(),
        ),
        notes: vec![format!(
            "graph auto-refreshed ({refreshed_files} files in {duration_ms}ms)"
        )],
    })
}

fn build_missing_index(
    repo_root: &Path,
    store: &GraphStore,
    in_flight_wait_ms: u64,
) -> Result<RefreshOutcome, CodeGraphError> {
    let Some(lock) = RefreshLock::acquire_wait(repo_root, in_flight_wait_ms)? else {
        return Ok(RefreshOutcome {
            freshness: graph_freshness_payload(false, true, &[], 0),
            notes: vec!["graph index build in progress by another process".to_owned()],
        });
    };
    let started = Instant::now();
    let report = run_index_unlocked(repo_root)?;
    let duration_ms = started.elapsed().as_millis();
    drop(lock);

    let counts = store.counts()?;
    let ready = counts.files > 0;
    Ok(RefreshOutcome {
        freshness: graph_freshness_payload(
            ready,
            true,
            &[],
            store.failed_diagnostic_paths()?.len(),
        ),
        notes: vec![if ready {
            format!(
                "graph index built on demand ({} files in {duration_ms}ms)",
                report.indexed_files
            )
        } else {
            "graph index built on demand but found no indexable files".to_owned()
        }],
    })
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
