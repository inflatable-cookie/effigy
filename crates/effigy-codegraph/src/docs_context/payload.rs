//! Typed report for the bounded `effigy docs context` retrieval surface.
//!
//! The payload is evidence, not an answer: every entry carries the exact
//! repository span it was read from so a caller can verify or re-read it.

use serde::{Deserialize, Serialize};

use crate::json::GraphFreshnessPayload;
use crate::model::{Provenance, SourceSpan};

/// Versioned schema identifier for `effigy docs context --json`.
pub const DOCS_CONTEXT_SCHEMA: &str = "effigy.docs.context.v1";
/// Schema version carried inside [`DocsContextPayload`].
pub const DOCS_CONTEXT_SCHEMA_VERSION: u8 = 1;

/// Default retrieval budgets from contract `041`.
pub const DEFAULT_MAX_SECTIONS: usize = 8;
pub const DEFAULT_MAX_BYTES: usize = 24_000;
pub const DEFAULT_MAX_HOPS: usize = 1;

/// Hard retrieval ceilings from contract `041`.
pub const MAX_MAX_SECTIONS: usize = 32;
pub const MAX_MAX_BYTES: usize = 100_000;
pub const MAX_MAX_HOPS: usize = 3;

/// Requested budgets as supplied by the caller; `None` means "use the default".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocsContextRequest {
    pub max_sections: Option<usize>,
    pub max_bytes: Option<usize>,
    pub max_hops: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextBudgetSetPayload {
    pub max_sections: usize,
    pub max_bytes: usize,
    pub max_hops: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextRequestedBudgetsPayload {
    pub max_sections: Option<usize>,
    pub max_bytes: Option<usize>,
    pub max_hops: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextBudgetsPayload {
    pub requested: DocsContextRequestedBudgetsPayload,
    pub applied: DocsContextBudgetSetPayload,
    pub defaults: DocsContextBudgetSetPayload,
    pub maximum: DocsContextBudgetSetPayload,
}

/// Repository-owned documentation profile state joined to this query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextProfilePayload {
    /// `baseline` when `[docs_policy.graph]` is absent, otherwise `configured`.
    pub state: String,
    /// Normalized compiled-profile fingerprint shared with graph freshness.
    pub fingerprint: String,
    pub roots: Vec<String>,
    pub fields: Vec<String>,
    pub kinds: Vec<String>,
    pub relations: Vec<String>,
    /// Markdown documents inside the retrieval scope.
    pub scoped_documents: usize,
}

/// One query term with the corpus evidence used to weight it.
///
/// A term present in most of the scoped corpus is reported with
/// `weighted = false`: it stayed out of scoring so it could not pull unrelated
/// sections into the report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextTermPayload {
    pub term: String,
    pub document_frequency: usize,
    pub weighted: bool,
}

/// One extracted `Label: value` fact carried with its exact source span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextFactPayload {
    pub field: String,
    pub value: String,
    pub span: SourceSpan,
}

/// One traversed typed-relation edge on the path from a lexical seed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextRelationStepPayload {
    pub relation: String,
    pub from_path: String,
    pub to_path: String,
    pub target: String,
    pub span: Option<SourceSpan>,
}

/// One deduplicated section of exact repository source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextResultPayload {
    pub rank: usize,
    pub record_id: String,
    pub path: String,
    pub heading: Option<String>,
    pub anchor: Option<String>,
    /// `heading-h<N>` for a heading section, `document` for a whole file.
    pub section_kind: String,
    /// Repository-defined document kind, or `document` in baseline mode.
    pub document_kind: String,
    pub authority: i64,
    /// `current`, `historical`, or `unknown`.
    pub currentness: String,
    pub span: SourceSpan,
    pub bytes: usize,
    /// Exact repository text for [`Self::span`].
    pub source: String,
    pub fields: Vec<DocsContextFactPayload>,
    pub hops: usize,
    pub relation_path: Vec<DocsContextRelationStepPayload>,
    /// `lexical` for a direct query match, `relation` for a traversed result.
    pub match_kind: String,
    pub match_reasons: Vec<String>,
    pub relevance: i64,
    pub provenance: Provenance,
}

/// Why the report stopped, and what was left out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextTruncationPayload {
    pub truncated: bool,
    pub section_budget_reached: bool,
    pub byte_budget_reached: bool,
    pub hop_budget_reached: bool,
    pub omitted_sections: usize,
    pub used_bytes: usize,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextDiagnosticPayload {
    pub severity: String,
    pub message: String,
    pub path: Option<String>,
    pub span: Option<SourceSpan>,
}

/// `effigy docs context --json` payload (`effigy.docs.context.v1`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocsContextPayload {
    pub schema: String,
    pub schema_version: u8,
    pub query: String,
    pub repo_root: String,
    pub profile: DocsContextProfilePayload,
    pub freshness: GraphFreshnessPayload,
    pub budgets: DocsContextBudgetsPayload,
    pub terms: Vec<DocsContextTermPayload>,
    pub results: Vec<DocsContextResultPayload>,
    pub truncation: DocsContextTruncationPayload,
    pub diagnostics: Vec<DocsContextDiagnosticPayload>,
    pub next: Vec<String>,
}

impl DocsContextBudgetSetPayload {
    pub(super) fn defaults() -> Self {
        Self {
            max_sections: DEFAULT_MAX_SECTIONS,
            max_bytes: DEFAULT_MAX_BYTES,
            max_hops: DEFAULT_MAX_HOPS,
        }
    }

    pub(super) fn maximum() -> Self {
        Self {
            max_sections: MAX_MAX_SECTIONS,
            max_bytes: MAX_MAX_BYTES,
            max_hops: MAX_MAX_HOPS,
        }
    }
}
