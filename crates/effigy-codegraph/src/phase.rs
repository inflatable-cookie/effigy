//! What the graph was doing when a wall-clock bound expired.
//!
//! A bounded graph command detaches its worker on timeout, so the reporting
//! thread cannot ask the worker what it reached. Instead the worker records
//! its current phase — and, where the phase is file-proportional, how far it
//! got — into process-global atomics that any thread can read at any time.
//! The record is advisory diagnostics only: nothing branches on it, and a
//! torn read costs a slightly stale label, never correctness.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::Instant;

use serde::Serialize;

/// A named span of graph work, ordered as the refresh and query pipeline runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphPhase {
    /// Nothing recorded yet in this process.
    Idle,
    /// Deciding whether the stored index still matches the working tree.
    FreshnessScan,
    /// Waiting for another process to release the cross-process refresh lock.
    RefreshLockWait,
    /// Enumerating repository files for a rebuild.
    IndexWalk,
    /// Extracting and storing per-file graph records.
    IndexFiles,
    /// Rebuilding the shared full-text search table.
    SearchIndexRebuild,
    /// Loading the documents, sections, facts, and relations a query may see.
    DocsScope,
    /// Scoring and traversing candidates.
    DocsRank,
    /// Applying section and byte budgets to the selected evidence.
    DocsSelect,
}

impl GraphPhase {
    /// Stable wire name. Kept kebab-case so it reads the same in JSON and text.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::FreshnessScan => "freshness-scan",
            Self::RefreshLockWait => "refresh-lock-wait",
            Self::IndexWalk => "index-walk",
            Self::IndexFiles => "index-files",
            Self::SearchIndexRebuild => "search-index-rebuild",
            Self::DocsScope => "docs-scope",
            Self::DocsRank => "docs-rank",
            Self::DocsSelect => "docs-select",
        }
    }

    fn from_code(code: u64) -> Self {
        match code {
            1 => Self::FreshnessScan,
            2 => Self::RefreshLockWait,
            3 => Self::IndexWalk,
            4 => Self::IndexFiles,
            5 => Self::SearchIndexRebuild,
            6 => Self::DocsScope,
            7 => Self::DocsRank,
            8 => Self::DocsSelect,
            _ => Self::Idle,
        }
    }

    fn code(self) -> u64 {
        match self {
            Self::Idle => 0,
            Self::FreshnessScan => 1,
            Self::RefreshLockWait => 2,
            Self::IndexWalk => 3,
            Self::IndexFiles => 4,
            Self::SearchIndexRebuild => 5,
            Self::DocsScope => 6,
            Self::DocsRank => 7,
            Self::DocsSelect => 8,
        }
    }
}

/// The phase a reader observed, with how long it had been running and how much
/// file-proportional work it had completed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphPhaseSnapshot {
    /// Stable phase name; see [`GraphPhase::name`].
    pub name: String,
    /// Milliseconds since this phase was entered.
    pub elapsed_ms: u64,
    /// Files completed in this phase, when the phase counts files.
    pub items_done: Option<usize>,
    /// Files this phase expects to process, when that total is known.
    pub items_total: Option<usize>,
}

static PHASE: AtomicU64 = AtomicU64::new(0);
static ENTERED_MICROS: AtomicU64 = AtomicU64::new(0);
static ITEMS_DONE: AtomicUsize = AtomicUsize::new(0);
static ITEMS_TOTAL: AtomicUsize = AtomicUsize::new(0);

/// Process start, so phase timestamps are a monotonic offset rather than a
/// wall clock a user could move underneath us.
fn origin() -> Instant {
    static ORIGIN: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    *ORIGIN.get_or_init(Instant::now)
}

/// Record that graph work has entered `phase`, clearing any item counters.
pub(crate) fn enter(phase: GraphPhase) {
    ITEMS_DONE.store(0, Ordering::Relaxed);
    ITEMS_TOTAL.store(0, Ordering::Relaxed);
    ENTERED_MICROS.store(origin().elapsed().as_micros() as u64, Ordering::Relaxed);
    PHASE.store(phase.code(), Ordering::Relaxed);
}

/// Record that `phase` began and expects `total` items.
pub(crate) fn enter_with_total(phase: GraphPhase, total: usize) {
    enter(phase);
    ITEMS_TOTAL.store(total, Ordering::Relaxed);
}

/// Record one completed item in the current phase.
pub(crate) fn item_done() {
    ITEMS_DONE.fetch_add(1, Ordering::Relaxed);
}

/// What the graph is doing right now, or `None` before any phase was entered.
pub fn snapshot() -> Option<GraphPhaseSnapshot> {
    let phase = GraphPhase::from_code(PHASE.load(Ordering::Relaxed));
    if phase == GraphPhase::Idle {
        return None;
    }
    let entered = ENTERED_MICROS.load(Ordering::Relaxed);
    let now = origin().elapsed().as_micros() as u64;
    let items_total = ITEMS_TOTAL.load(Ordering::Relaxed);
    Some(GraphPhaseSnapshot {
        name: phase.name().to_owned(),
        elapsed_ms: now.saturating_sub(entered) / 1_000,
        items_done: (items_total > 0).then(|| ITEMS_DONE.load(Ordering::Relaxed)),
        items_total: (items_total > 0).then_some(items_total),
    })
}

/// Every phase name this crate can report, so callers and tests can check a
/// reported name against the closed set instead of restating it.
pub const KNOWN_PHASE_NAMES: &[&str] = &[
    "freshness-scan",
    "refresh-lock-wait",
    "index-walk",
    "index-files",
    "search-index-rebuild",
    "docs-scope",
    "docs-rank",
    "docs-select",
];

#[cfg(test)]
mod tests {
    use super::{GraphPhase, KNOWN_PHASE_NAMES};

    /// The wire name is the only part of a phase a consumer can see, so every
    /// reportable phase must round-trip its code and appear in the closed set.
    #[test]
    fn every_reportable_phase_round_trips_and_is_named() {
        let reportable = [
            GraphPhase::FreshnessScan,
            GraphPhase::RefreshLockWait,
            GraphPhase::IndexWalk,
            GraphPhase::IndexFiles,
            GraphPhase::SearchIndexRebuild,
            GraphPhase::DocsScope,
            GraphPhase::DocsRank,
            GraphPhase::DocsSelect,
        ];
        assert_eq!(reportable.len(), KNOWN_PHASE_NAMES.len());
        for phase in reportable {
            assert_eq!(GraphPhase::from_code(phase.code()), phase);
            assert!(KNOWN_PHASE_NAMES.contains(&phase.name()));
        }
        assert_eq!(GraphPhase::Idle.name(), "idle");
        assert_eq!(
            GraphPhase::from_code(GraphPhase::Idle.code()),
            GraphPhase::Idle
        );
        assert_eq!(GraphPhase::from_code(u64::MAX), GraphPhase::Idle);
    }
}
