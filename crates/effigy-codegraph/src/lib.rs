//! Native code-graph indexing and query support for Effigy.
//!
//! This crate owns the local graph artifact under `.effigy/graph/graph.db`,
//! first-party language extraction, graph-query helpers, and watch-mode refresh.
//! The SQLite store is an implementation detail. The CLI JSON payloads exposed
//! through `effigy graph ... --json` are the stable machine-facing contract.
//!
//! Typical entry points:
//! - [`run_index`] to build or refresh the graph
//! - [`status`] to inspect freshness and graph counts
//! - [`query_search`], [`node`], [`callers`], [`callees`], [`impact`], and
//!   [`context`] to read the graph
//! - [`explore`] to assemble a one-call agent navigation packet with primary
//!   owners, excerpts, related symbols, and fallback guidance
//! - [`affected`] to narrow likely validation scope from changed-file input
//! - [`watch_repo`] to keep the graph fresh from foreground filesystem events
//!
//! Recommended agent workflow:
//! 1. query functions refresh a stale or missing index on demand, so graph
//!    reads are current without a manual `graph index` step
//! 2. run [`status`] to inspect freshness and counts; it stays report-only and
//!    never mutates graph state
//! 3. start task-shaped navigation with [`explore`]
//! 4. use [`affected`] when the question is which tests or tasks to run after edits
//! 5. fall back to [`context`] or exact-search tools for lower-level confirmation

mod error;
pub mod extractor;
mod git;
mod ids;
pub mod index;
pub mod json;
mod language;
pub mod model;
pub mod paths;
pub mod query;
pub mod refresh;
mod registry;
pub mod storage;
mod support;
mod walk;
pub mod watch;

/// Shared error type for graph indexing, query, storage, and watch failures.
pub use error::CodeGraphError;
/// Stable extractor identity for stored graph facts.
pub use ids::{ExtractorId, GraphId};
/// Build or refresh the local graph, then inspect freshness and counts.
pub use index::{run_index, status, status_with_refresh, IndexReport};
/// Render graph payloads into the public JSON contract.
pub use json::{render_json, GraphCommandPayload, GRAPH_JSON_SCHEMA_VERSION};
/// Query helpers over the stored graph.
pub use query::{
    affected, callees, callers, context, explore, files as query_files, impact, node,
    search as query_search,
};
/// Lazy on-query graph refresh (rebuilds stale indexes on demand).
pub use refresh::{ensure_fresh, RefreshOutcome};
/// Local SQLite-backed graph store.
pub use storage::GraphStore;
/// Foreground graph watch surface and typed watch events.
pub use watch::{watch_repo, GraphWatchEvent, GraphWatchOptions};

#[cfg(test)]
mod tests;
