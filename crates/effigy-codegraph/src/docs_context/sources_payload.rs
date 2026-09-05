//! Typed report for cross-repository `effigy docs context --sources`.
//!
//! Distinct from `effigy.docs.context.v1` on purpose: the single-repository
//! payload answers "what does this repository say", and merging repository
//! blocks into it would invite a merged ranking. Results stay grouped, and
//! authority is never compared across repositories.

use serde::{Deserialize, Serialize};

use crate::json::GraphFreshnessPayload;

use super::payload::{DocsContextBudgetsPayload, DocsContextResultPayload};

/// Versioned schema identifier for `effigy docs context --sources --json`.
pub const DOCS_CONTEXT_SOURCES_SCHEMA: &str = "effigy.docs.context.sources.v1";
/// Schema version carried inside [`DocsContextSourcesPayload`].
pub const DOCS_CONTEXT_SOURCES_SCHEMA_VERSION: u8 = 1;

/// Per-repository outcome vocabulary. Every value except `ok` and `empty`
/// carries a next step, and no value hides another repository's results.
pub const STATUS_OK: &str = "ok";
pub const STATUS_EMPTY: &str = "empty";
pub const STATUS_STALE: &str = "stale";
pub const STATUS_TIMEOUT: &str = "timeout";
pub const STATUS_NOT_SHARED: &str = "not-shared";
pub const STATUS_MISSING: &str = "missing";
pub const STATUS_INVALID: &str = "invalid";
pub const STATUS_DISALLOWED: &str = "disallowed";

/// Whether a returned excerpt is the committed bytes at HEAD or working-tree
/// content. Never inferred optimistically: any uncertainty reports
/// [`CONTENT_IDENTITY_WORKING_TREE`].
pub const CONTENT_IDENTITY_COMMITTED: &str = "committed";
pub const CONTENT_IDENTITY_WORKING_TREE: &str = "working-tree";

/// One section of exact repository source plus the identity of the bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextSourceResultPayload {
    /// Every field of the single-repository result, unchanged.
    #[serde(flatten)]
    pub result: DocsContextResultPayload,
    /// `committed` when the file matches HEAD, otherwise `working-tree`.
    pub content_identity: String,
}

/// One repository block, in portfolio directory order then child-name order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextRepositoryPayload {
    /// Directory name of the checkout; the handle `--only` selects on.
    pub handle: String,
    /// Absolute path, or `None` for a handle that resolved to nothing.
    pub path: Option<String>,
    /// Portfolio directory this repository was enumerated from.
    pub directory: Option<String>,
    /// One of the status vocabulary constants in this module.
    pub status: String,
    /// Present for every non-`ok` status; what the operator should do next.
    pub next_step: Option<String>,
    /// `git rev-parse HEAD` for the checkout, when git can answer.
    pub current_head: Option<String>,
    /// HEAD the local graph index was built from, when it was built clean.
    pub indexed_head: Option<String>,
    pub freshness: Option<GraphFreshnessPayload>,
    /// `baseline` or `configured`, from the repository's own profile.
    pub profile_state: Option<String>,
    /// Repository-declared entry points, as written in its manifest.
    pub front_doors: Vec<String>,
    /// Repository-declared skill directories, as written in its manifest.
    pub skill_roots: Vec<String>,
    pub results: Vec<DocsContextSourceResultPayload>,
}

/// `effigy docs context --sources --json` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextSourcesPayload {
    pub schema: String,
    pub schema_version: u8,
    pub query: String,
    /// The portfolio file, or the directory passed to `--sources`.
    pub portfolio_path: String,
    /// Directories named by the portfolio, as written.
    pub directories: Vec<String>,
    /// Handles requested with `--only`, in the order given.
    pub only: Vec<String>,
    /// Budgets each repository received in full; they are never divided.
    pub budgets: DocsContextBudgetsPayload,
    pub repositories: Vec<DocsContextRepositoryPayload>,
    pub next: Vec<String>,
}

impl DocsContextSourcesPayload {
    /// True when at least one repository answered, which is the exit-0 rule.
    ///
    /// A degraded neighbour is reported, not fatal: the caller still got
    /// evidence, and hiding it behind another repository's failure is the
    /// behavior this surface exists to avoid.
    pub fn answered(&self) -> bool {
        self.repositories
            .iter()
            .any(|repository| repository.status == STATUS_OK || repository.status == STATUS_EMPTY)
    }
}
