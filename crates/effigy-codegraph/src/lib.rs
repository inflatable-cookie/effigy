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
//! - [`watch_repo`] to keep the graph fresh from foreground filesystem events

mod error;
pub mod extractor;
mod ids;
pub mod index;
pub mod json;
mod language;
pub mod model;
pub mod paths;
pub mod query;
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
pub use index::{run_index, status, IndexReport};
/// Render graph payloads into the public JSON contract.
pub use json::{render_json, GraphCommandPayload, GRAPH_JSON_SCHEMA_VERSION};
/// Query helpers over the stored graph.
pub use query::{
    callees, callers, context, files as query_files, impact, node, search as query_search,
};
/// Local SQLite-backed graph store.
pub use storage::GraphStore;
/// Foreground graph watch surface and typed watch events.
pub use watch::{watch_repo, GraphWatchEvent, GraphWatchOptions};

#[cfg(test)]
mod tests;
