//! `effigy docs context` shell: root selection, rendering, and exit behavior.
//!
//! Ranking, traversal, and budgeting belong to `effigy-codegraph`. This module
//! only turns one typed report into text or the versioned JSON payload. The
//! lazy graph refresh inside that retrieval shares the graph command's
//! wall-clock budget, typed timeout detail, and cold/stale progress notice.

use std::path::Path;

use effigy_codegraph::docs_context::{
    DocsContextPayload, DocsContextRequest, DocsContextResultPayload,
};
use effigy_codegraph::RefreshPending;

use crate::runner::render::render_command_result;

use super::RunnerError;

/// Command identity reused by the shared graph timeout detail.
const DOCS_CONTEXT_COMMAND: &str = "docs context";

pub(super) fn run_context(
    repo_root: &Path,
    query: &str,
    max_sections: Option<usize>,
    max_bytes: Option<usize>,
    max_hops: Option<usize>,
    output_json: bool,
) -> Result<String, RunnerError> {
    // An empty query can never refresh anything; `docs_context` rejects it
    // directly, so skip the progress probe instead of claiming a refresh.
    if !query.trim().is_empty() {
        if let Some(notice) = refresh_progress_notice(repo_root) {
            eprintln!("{notice}");
        }
    }
    let owned_root = repo_root.to_path_buf();
    let owned_query = query.to_owned();
    match super::super::graph_time_budget::graph_time_budget() {
        Some(budget) => {
            let worker_root = owned_root.clone();
            super::super::graph_time_budget::run_bounded_graph_operation(
                &owned_root,
                DOCS_CONTEXT_COMMAND,
                budget,
                move || {
                    retrieve_documentation_context(
                        &worker_root,
                        &owned_query,
                        max_sections,
                        max_bytes,
                        max_hops,
                        output_json,
                    )
                },
            )
        }
        None => retrieve_documentation_context(
            repo_root,
            query,
            max_sections,
            max_bytes,
            max_hops,
            output_json,
        ),
    }
}

/// Decide the stderr progress notice for this query's lazy refresh.
///
/// `Some` only when the next refresh would actually do work — a cold build or
/// a stale rebuild — so warm and current queries never claim one. Probe
/// failures stay silent: the retrieval itself surfaces the real error.
pub(super) fn refresh_progress_notice(repo_root: &Path) -> Option<String> {
    match effigy_codegraph::refresh_pending(repo_root) {
        Ok(RefreshPending::Cold) => Some(format!(
            "[docs] {DOCS_CONTEXT_COMMAND}: graph index is missing; building the shared graph index before answering"
        )),
        Ok(RefreshPending::Stale) => Some(format!(
            "[docs] {DOCS_CONTEXT_COMMAND}: graph index is stale; refreshing it before answering"
        )),
        Ok(RefreshPending::Current) | Err(_) => None,
    }
}

fn retrieve_documentation_context(
    repo_root: &Path,
    query: &str,
    max_sections: Option<usize>,
    max_bytes: Option<usize>,
    max_hops: Option<usize>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let payload = effigy_codegraph::docs_context(
        repo_root,
        query,
        DocsContextRequest {
            max_sections,
            max_bytes,
            max_hops,
        },
    )
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    let text = render_context_text(&payload);
    let json = serde_json::to_value(&payload)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    render_command_result(output_json, true, json, text)
}

fn render_context_text(payload: &DocsContextPayload) -> String {
    let mut lines = vec![
        format!("docs context `{}`", payload.query),
        format!(
            "profile: {} ({} scoped documents, fingerprint {})",
            payload.profile.state,
            payload.profile.scoped_documents,
            short_fingerprint(&payload.profile.fingerprint)
        ),
        format!(
            "graph: {} - {}",
            payload.freshness.state, payload.freshness.summary
        ),
        format!(
            "budgets: sections {}/{}, bytes {}/{}, hops {}",
            payload.results.len(),
            payload.budgets.applied.max_sections,
            payload.truncation.used_bytes,
            payload.budgets.applied.max_bytes,
            payload.budgets.applied.max_hops
        ),
        format!("results: {}", payload.results.len()),
    ];

    if payload.results.is_empty() {
        lines.push(String::new());
        lines.push(if payload.truncation.omitted_sections == 0 {
            "no in-scope documentation section matched this query".to_owned()
        } else {
            format!(
                "{} matching section(s) did not fit the requested budgets",
                payload.truncation.omitted_sections
            )
        });
    }
    for result in &payload.results {
        lines.push(String::new());
        lines.extend(render_result(result));
    }

    if payload.truncation.truncated {
        lines.push(String::new());
        lines.push(format!(
            "truncation: {} omitted section(s){}",
            payload.truncation.omitted_sections,
            if payload.truncation.hop_budget_reached {
                "; hop budget reached"
            } else {
                ""
            }
        ));
        for reason in &payload.truncation.reasons {
            lines.push(format!("- {reason}"));
        }
    }

    if !payload.diagnostics.is_empty() {
        lines.push(String::new());
        lines.push("diagnostics:".to_owned());
        for diagnostic in &payload.diagnostics {
            lines.push(format!("- {}: {}", diagnostic.severity, diagnostic.message));
        }
    }

    if !payload.next.is_empty() {
        lines.push(String::new());
        lines.push("next:".to_owned());
        for step in &payload.next {
            lines.push(format!("- {step}"));
        }
    }

    lines.join("\n")
}

fn render_result(result: &DocsContextResultPayload) -> Vec<String> {
    let anchor = result
        .anchor
        .as_deref()
        .filter(|anchor| !anchor.is_empty())
        .map(|anchor| format!("#{anchor}"))
        .unwrap_or_default();
    let mut lines = vec![
        format!(
            "{}. {}{} [{}] authority {} currentness {}",
            result.rank,
            result.path,
            anchor,
            result.document_kind,
            result.authority,
            result.currentness
        ),
        format!(
            "   lines {}-{} bytes {}-{} ({} bytes) {} hop(s) via {}",
            result.span.start.line,
            result.span.end.line,
            result.span.start.byte,
            result.span.end.byte,
            result.bytes,
            result.hops,
            result.match_kind
        ),
    ];
    if result.seed_path != result.path {
        lines.push(format!("   seed: {}", result.seed_path));
    }
    if !result.match_reasons.is_empty() {
        lines.push(format!("   match: {}", result.match_reasons.join("; ")));
    }
    if !result.fields.is_empty() {
        let fields = result
            .fields
            .iter()
            .map(|fact| format!("{}={}", fact.field, fact.value))
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!("   fields: {fields}"));
    }
    if !result.relation_path.is_empty() {
        let path = result
            .relation_path
            .iter()
            .map(|step| format!("{} -[{}]-> {}", step.from_path, step.relation, step.to_path))
            .collect::<Vec<_>>()
            .join(" | ");
        lines.push(format!("   relations: {path}"));
    }
    lines.push("   ---".to_owned());
    for line in result.source.lines() {
        lines.push(format!("   {line}"));
    }
    lines
}

fn short_fingerprint(fingerprint: &str) -> String {
    fingerprint.chars().take(12).collect()
}
