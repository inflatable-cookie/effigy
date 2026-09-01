//! Bounded documentation context retrieval (`effigy docs context`).
//!
//! The query is evidence selection over the shared graph, never synthesis: it
//! returns exact repository sections with provenance and stops inside explicit
//! section, byte, and hop budgets. Repository vocabulary comes from the
//! optional `[docs_policy.graph]` profile; nothing here knows any specific
//! repository's kinds, statuses, paths, or relations.

use std::collections::BTreeMap;
use std::path::Path;

use crate::docs_profile::{load_docs_profile_state, CompiledDocsProfile, DocsProfileState};
use crate::error::CodeGraphError;
use crate::json::GraphFreshnessPayload;
use crate::model::{DiagnosticSeverity, SourceSpan};
use crate::refresh::RefreshPending;
use crate::storage::GraphStore;

mod payload;
mod rank;
mod scope;

pub use payload::{
    DocsContextBudgetSetPayload, DocsContextBudgetsPayload, DocsContextDiagnosticPayload,
    DocsContextFactPayload, DocsContextPayload, DocsContextProfilePayload,
    DocsContextRelationStepPayload, DocsContextRequest, DocsContextRequestedBudgetsPayload,
    DocsContextResultPayload, DocsContextTermPayload, DocsContextTruncationPayload,
    DEFAULT_MAX_BYTES, DEFAULT_MAX_HOPS, DEFAULT_MAX_SECTIONS, DOCS_CONTEXT_SCHEMA,
    DOCS_CONTEXT_SCHEMA_VERSION, MAX_MAX_BYTES, MAX_MAX_HOPS, MAX_MAX_SECTIONS,
};

use rank::Candidate;
use scope::{collect_scope, DocsScope, ScopedDocument};

const PROFILE_STATE_BASELINE: &str = "baseline";
const PROFILE_STATE_CONFIGURED: &str = "configured";
const DOCUMENT_SECTION_KIND: &str = "document";
/// Bound on named truncation reasons so a tiny budget cannot produce a report
/// that is mostly omission notes.
const MAX_TRUNCATION_REASONS: usize = 8;

/// Retrieve bounded documentation evidence for `query`.
///
/// Refreshes the shared graph through the existing lazy path, then selects
/// deduplicated exact sections under the requested budgets. A query that
/// matches nothing is a successful empty report; an empty query is an error.
pub fn docs_context(
    repo_root: &Path,
    query: &str,
    request: DocsContextRequest,
) -> Result<DocsContextPayload, CodeGraphError> {
    docs_context_with_progress(repo_root, query, request, |_| {})
}

/// Validate the query and budgets without touching graph state.
///
/// Callers that run graph work under a wall-clock bound invoke this on their
/// own thread first, so usage errors can never be pre-empted by a timeout.
/// [`docs_context`] runs the same validation because it is pure; there is one
/// validation model.
pub fn validate_docs_context_request(
    query: &str,
    request: DocsContextRequest,
) -> Result<DocsContextBudgetSetPayload, CodeGraphError> {
    let query = query.trim();
    if query.is_empty() {
        return Err(CodeGraphError::validation(
            "`docs context` requires a non-empty query",
        ));
    }
    resolve_budgets(request)
}

/// [`docs_context`] with a progress callback for the lazy refresh.
///
/// The callback receives the refresh verdict before any rebuild walk starts —
/// `Cold` before a missing index is built, `Stale` after the freshness scan
/// finds changed files and before the rebuild — so a caller can announce
/// progress while the refresh is still inside its own bound. Current graphs
/// produce no callback.
pub fn docs_context_with_progress(
    repo_root: &Path,
    query: &str,
    request: DocsContextRequest,
    progress: impl FnMut(RefreshPending),
) -> Result<DocsContextPayload, CodeGraphError> {
    let applied = validate_docs_context_request(query, request)?;
    let query = query.trim();

    let store = GraphStore::open(repo_root)?;
    let freshness = ensure_freshness(repo_root, &store, progress)?;
    let profile_state = load_docs_profile_state(repo_root)?;
    let scope = collect_scope(&store, &profile_state)?;

    let ranked = rank::rank(repo_root, &store, &scope, query, applied.max_hops)?;
    let deduplicated = deduplicate(&scope, ranked.candidates);
    let selection = select(repo_root, &scope, &ranked.contents, &deduplicated, applied);

    let mut diagnostics = collect_diagnostics(&store, &scope, &selection.results)?;
    for path in &ranked.unreadable_paths {
        diagnostics.push(DocsContextDiagnosticPayload {
            severity: "warning".to_owned(),
            message: format!("indexed document `{path}` could not be read from disk"),
            path: Some(path.clone()),
            span: None,
        });
    }

    let profile = profile_payload(&profile_state, &scope);
    let mut truncation = DocsContextTruncationPayload {
        truncated: selection.section_budget_reached
            || selection.byte_budget_reached
            || ranked.hop_budget_reached,
        section_budget_reached: selection.section_budget_reached,
        byte_budget_reached: selection.byte_budget_reached,
        hop_budget_reached: ranked.hop_budget_reached,
        omitted_sections: selection.omitted_sections,
        used_bytes: selection.used_bytes,
        reasons: selection.reasons.clone(),
    };
    if truncation.hop_budget_reached {
        truncation.reasons.push(format!(
            "hop budget reached at {} hop(s): further typed relations were not traversed",
            applied.max_hops
        ));
    }
    let next = next_steps(&profile, &freshness, &selection, &truncation, &diagnostics);

    Ok(DocsContextPayload {
        schema: DOCS_CONTEXT_SCHEMA.to_owned(),
        schema_version: DOCS_CONTEXT_SCHEMA_VERSION,
        query: query.to_owned(),
        repo_root: repo_root.display().to_string(),
        profile,
        freshness,
        budgets: DocsContextBudgetsPayload {
            requested: DocsContextRequestedBudgetsPayload {
                max_sections: request.max_sections,
                max_bytes: request.max_bytes,
                max_hops: request.max_hops,
            },
            applied,
            defaults: DocsContextBudgetSetPayload::defaults(),
            maximum: DocsContextBudgetSetPayload::maximum(),
        },
        terms: ranked
            .terms
            .into_iter()
            .map(|term| DocsContextTermPayload {
                term: term.term,
                document_frequency: term.document_frequency,
                weighted: term.weighted,
            })
            .collect(),
        results: selection.results,
        truncation,
        diagnostics,
        next,
    })
}

fn resolve_budgets(
    request: DocsContextRequest,
) -> Result<DocsContextBudgetSetPayload, CodeGraphError> {
    Ok(DocsContextBudgetSetPayload {
        max_sections: bounded(
            "--max-sections",
            request.max_sections,
            DEFAULT_MAX_SECTIONS,
            MAX_MAX_SECTIONS,
        )?,
        max_bytes: bounded(
            "--max-bytes",
            request.max_bytes,
            DEFAULT_MAX_BYTES,
            MAX_MAX_BYTES,
        )?,
        max_hops: bounded(
            "--max-hops",
            request.max_hops,
            DEFAULT_MAX_HOPS,
            MAX_MAX_HOPS,
        )?,
    })
}

fn bounded(
    flag: &str,
    requested: Option<usize>,
    default: usize,
    maximum: usize,
) -> Result<usize, CodeGraphError> {
    let Some(value) = requested else {
        return Ok(default);
    };
    if value == 0 {
        return Err(CodeGraphError::validation(format!(
            "`{flag}` must be greater than 0"
        )));
    }
    if value > maximum {
        return Err(CodeGraphError::validation(format!(
            "`{flag}` must be at most {maximum}"
        )));
    }
    Ok(value)
}

fn ensure_freshness(
    repo_root: &Path,
    store: &GraphStore,
    progress: impl FnMut(RefreshPending),
) -> Result<GraphFreshnessPayload, CodeGraphError> {
    let outcome = crate::refresh::ensure_fresh_with_progress(repo_root, store, progress)?;
    let mut freshness = outcome.freshness;
    if !outcome.notes.is_empty() {
        freshness.summary = format!("{} ({})", freshness.summary, outcome.notes.join("; "));
    }
    Ok(freshness)
}

/// Drop any candidate whose span is already covered by a higher-ranked
/// candidate from the same document, so nested headings are not returned twice.
fn deduplicate(scope: &DocsScope, candidates: Vec<Candidate>) -> Vec<Candidate> {
    let mut kept: Vec<Candidate> = Vec::new();
    let mut spans: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    for candidate in candidates {
        let Some(span) = candidate_span(scope, &candidate) else {
            continue;
        };
        let range = (span.start.byte, span.end.byte);
        let entry = spans.entry(candidate.path.clone()).or_default();
        if entry
            .iter()
            .any(|(start, end)| range.0 < *end && *start < range.1)
        {
            continue;
        }
        entry.push(range);
        kept.push(candidate);
    }
    kept
}

struct Selection {
    results: Vec<DocsContextResultPayload>,
    used_bytes: usize,
    omitted_sections: usize,
    section_budget_reached: bool,
    byte_budget_reached: bool,
    reasons: Vec<String>,
}

/// Fill the report in rank order under the section and byte budgets.
///
/// A section that does not fit is omitted whole and named in the truncation
/// reasons; a partial section would read as complete evidence and mislead the
/// caller. Skipping one oversized section never reorders the sections that are
/// returned, so ordering stays relevance-first and reproducible.
///
/// When `max-sections >= 2` and a traversed candidate exists, the last slot is
/// held for the highest-ranked whole traversed section that fits. That keeps
/// 0-hop lexical saturation from consuming every slot without introducing a
/// second ranker or leaving an empty hole when nothing traversed fits.
fn select(
    repo_root: &Path,
    scope: &DocsScope,
    contents: &BTreeMap<String, String>,
    candidates: &[Candidate],
    budgets: DocsContextBudgetSetPayload,
) -> Selection {
    let mut buf = SelectBuf {
        repo_root,
        scope,
        contents,
        budgets,
        cache: BTreeMap::new(),
        results: Vec::new(),
        used_bytes: 0,
        section_budget_reached: false,
        byte_budget_reached: false,
        reasons: Vec::new(),
    };
    let mut pending_traversal =
        budgets.max_sections >= 2 && candidates.iter().any(|candidate| candidate.hops > 0);

    for candidate in candidates {
        if buf.results.len() >= budgets.max_sections {
            buf.mark_section_budget();
            break;
        }
        if pending_traversal && candidate.hops == 0 && buf.results.len() + 1 == budgets.max_sections
        {
            pending_traversal = false;
            if buf.add_best_fitting_traversal(candidates) {
                buf.mark_section_budget();
                break;
            }
        }
        buf.try_add(candidate);
    }

    Selection {
        omitted_sections: candidates.len().saturating_sub(buf.results.len()),
        results: buf.results,
        used_bytes: buf.used_bytes,
        section_budget_reached: buf.section_budget_reached,
        byte_budget_reached: buf.byte_budget_reached,
        reasons: buf.reasons,
    }
}

struct SelectBuf<'a> {
    repo_root: &'a Path,
    scope: &'a DocsScope,
    contents: &'a BTreeMap<String, String>,
    budgets: DocsContextBudgetSetPayload,
    cache: BTreeMap<String, Option<String>>,
    results: Vec<DocsContextResultPayload>,
    used_bytes: usize,
    section_budget_reached: bool,
    byte_budget_reached: bool,
    reasons: Vec<String>,
}

impl SelectBuf<'_> {
    fn mark_section_budget(&mut self) {
        if self.section_budget_reached {
            return;
        }
        self.section_budget_reached = true;
        self.reasons.push(format!(
            "section budget reached after {} sections",
            self.budgets.max_sections
        ));
    }

    fn add_best_fitting_traversal(&mut self, candidates: &[Candidate]) -> bool {
        for candidate in candidates.iter().filter(|candidate| candidate.hops > 0) {
            if self.try_add(candidate) {
                return true;
            }
        }
        false
    }

    fn try_add(&mut self, candidate: &Candidate) -> bool {
        let indexed = self.contents.get(&candidate.path).cloned();
        let disk_path = self.repo_root.join(&candidate.path);
        let content = self
            .cache
            .entry(candidate.path.clone())
            .or_insert_with(|| indexed.or_else(|| std::fs::read_to_string(&disk_path).ok()))
            .clone();
        let Some(content) = content else {
            return false;
        };
        let Some(span) = candidate_span(self.scope, candidate) else {
            return false;
        };
        let Some(document) = self.scope.documents.get(&candidate.path) else {
            return false;
        };
        let source = slice(&content, span.start.byte, span.end.byte).to_owned();
        let bytes = source.len();
        if self.used_bytes + bytes > self.budgets.max_bytes {
            self.byte_budget_reached = true;
            if self.reasons.len() < MAX_TRUNCATION_REASONS {
                self.reasons.push(format!(
                    "byte budget omitted `{}`{}: {bytes} bytes exceed the remaining {}",
                    candidate.path,
                    candidate
                        .section
                        .and_then(|index| document.sections.get(index))
                        .map(|section| format!("#{}", section.anchor))
                        .unwrap_or_default(),
                    self.budgets.max_bytes.saturating_sub(self.used_bytes)
                ));
            }
            return false;
        }
        self.used_bytes += bytes;
        let rank = self.results.len() + 1;
        self.results.push(result_payload(
            document, candidate, span, source, bytes, rank,
        ));
        true
    }
}

fn result_payload(
    document: &ScopedDocument,
    candidate: &Candidate,
    span: SourceSpan,
    source: String,
    bytes: usize,
    rank: usize,
) -> DocsContextResultPayload {
    let section = candidate
        .section
        .and_then(|index| document.sections.get(index));
    DocsContextResultPayload {
        rank,
        record_id: candidate.record_id.clone(),
        path: document.path.clone(),
        heading: section.map(|section| section.heading.clone()),
        anchor: section.map(|section| section.anchor.clone()),
        section_kind: section
            .map(|section| section.kind.clone())
            .unwrap_or_else(|| DOCUMENT_SECTION_KIND.to_owned()),
        document_kind: document.document_kind.clone(),
        authority: document.authority,
        currentness: document.currentness.clone(),
        span,
        bytes,
        source,
        fields: document.facts.clone(),
        hops: candidate.hops,
        relation_path: candidate.relation_path.clone(),
        seed_path: candidate.seed_path.clone(),
        match_kind: candidate.match_kind.to_owned(),
        match_reasons: candidate.reasons.clone(),
        relevance: candidate.relevance,
        provenance: section
            .map(|section| section.provenance.clone())
            .unwrap_or_else(|| document.provenance.clone()),
    }
}

fn candidate_span(scope: &DocsScope, candidate: &Candidate) -> Option<SourceSpan> {
    let document = scope.documents.get(&candidate.path)?;
    match candidate.section {
        Some(index) => document
            .sections
            .get(index)
            .map(|section| section.span.clone()),
        None => Some(document.span.clone()),
    }
}

fn slice(content: &str, start: u32, end: u32) -> &str {
    let start = (start as usize).min(content.len());
    let end = (end as usize).clamp(start, content.len());
    content.get(start..end).unwrap_or_default()
}

/// Surface stored indexing diagnostics and unresolved typed links for the
/// documents this report actually returned.
fn collect_diagnostics(
    store: &GraphStore,
    scope: &DocsScope,
    results: &[DocsContextResultPayload],
) -> Result<Vec<DocsContextDiagnosticPayload>, CodeGraphError> {
    let mut paths = results
        .iter()
        .map(|result| result.path.clone())
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut diagnostics = Vec::new();
    for record in store.list_diagnostics()? {
        let path = record.provenance.source_path.clone();
        if !paths.contains(&path) {
            continue;
        }
        diagnostics.push(DocsContextDiagnosticPayload {
            severity: match record.severity {
                DiagnosticSeverity::Error => "error",
                DiagnosticSeverity::Warning => "warning",
                DiagnosticSeverity::Info => "info",
            }
            .to_owned(),
            message: record.message,
            path: Some(path),
            span: record.span,
        });
    }
    for path in &paths {
        for relation in scope.relations.get(path).into_iter().flatten() {
            if relation.target_id.is_some() {
                continue;
            }
            diagnostics.push(DocsContextDiagnosticPayload {
                severity: "warning".to_owned(),
                message: format!(
                    "relation `{}` in `{path}` has no in-repository target `{}`",
                    relation.relation, relation.target
                ),
                path: Some(path.clone()),
                span: relation.span.clone(),
            });
        }
    }
    Ok(diagnostics)
}

fn profile_payload(
    profile_state: &DocsProfileState,
    scope: &DocsScope,
) -> DocsContextProfilePayload {
    let compiled = profile_state.compiled();
    DocsContextProfilePayload {
        state: match compiled {
            Some(_) => PROFILE_STATE_CONFIGURED,
            None => PROFILE_STATE_BASELINE,
        }
        .to_owned(),
        fingerprint: profile_state.fingerprint(),
        roots: compiled.map(root_tokens).unwrap_or_default(),
        fields: compiled
            .map(|profile| profile.fields.keys().cloned().collect())
            .unwrap_or_default(),
        kinds: compiled
            .map(|profile| profile.kinds.keys().cloned().collect())
            .unwrap_or_default(),
        relations: compiled
            .map(|profile| profile.relations.keys().cloned().collect())
            .unwrap_or_default(),
        scoped_documents: scope.documents.len(),
    }
}

fn root_tokens(profile: &CompiledDocsProfile) -> Vec<String> {
    profile
        .roots
        .iter()
        .map(|root| root.relative.clone())
        .collect()
}

fn next_steps(
    profile: &DocsContextProfilePayload,
    freshness: &GraphFreshnessPayload,
    selection: &Selection,
    truncation: &DocsContextTruncationPayload,
    diagnostics: &[DocsContextDiagnosticPayload],
) -> Vec<String> {
    let mut next = Vec::new();
    if !freshness.usable {
        next.push("run `effigy graph index` to rebuild the shared graph".to_owned());
    }
    if selection.results.is_empty() && selection.omitted_sections == 0 {
        next.push(
            "no in-scope Markdown section matched; retry with terms that appear in the docs"
                .to_owned(),
        );
    }
    if selection.section_budget_reached {
        next.push("raise `--max-sections` to include more sections".to_owned());
    }
    if selection.byte_budget_reached {
        next.push("raise `--max-bytes` to include the next section whole".to_owned());
    }
    if truncation.hop_budget_reached {
        next.push("raise `--max-hops` to traverse further typed relations".to_owned());
    }
    if profile.state == PROFILE_STATE_BASELINE {
        next.push(
            "add `[docs_policy.graph]` to `effigy.toml` for repository-owned kinds, fields, and relations"
                .to_owned(),
        );
    }
    if diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == "error")
    {
        next.push("resolve the reported documentation graph errors before trusting authority and currentness".to_owned());
    }
    next
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
