//! `effigy docs context` shell: root selection, rendering, and exit behavior.
//!
//! Ranking, traversal, and budgeting belong to `effigy-codegraph`. This module
//! only turns one typed report into text or the versioned JSON payload. The
//! lazy graph refresh inside that retrieval shares the graph command's
//! wall-clock budget, typed timeout detail, and cold/stale progress notice.

use std::path::Path;

use effigy_codegraph::docs_context::{
    docs_context_sources, DocsContextPayload, DocsContextRepositoryPayload, DocsContextRequest,
    DocsContextResultPayload, DocsContextSourceResultPayload, DocsContextSourcesPayload,
    SourceQueryOutcome, STATUS_OK,
};
use effigy_codegraph::RefreshPending;

use crate::runner::render::render_command_result;

use super::RunnerError;

/// Command identity reused by the shared graph timeout detail.
const DOCS_CONTEXT_COMMAND: &str = "docs context";

#[allow(clippy::too_many_arguments)]
pub(super) fn run_context(
    repo_root: &Path,
    query: &str,
    max_sections: Option<usize>,
    max_bytes: Option<usize>,
    max_hops: Option<usize>,
    sources: Option<std::path::PathBuf>,
    only: Vec<String>,
    output_json: bool,
) -> Result<String, RunnerError> {
    let request = DocsContextRequest {
        max_sections,
        max_bytes,
        max_hops,
    };
    if let Some(portfolio) = sources {
        return run_context_sources(&portfolio, query, request, &only, output_json);
    }
    // Usage errors — empty query, invalid budgets — must win over the
    // wall-clock bound: they are validated here, on the caller thread, before
    // any bounded graph work starts. The same validation runs again inside
    // the retrieval because it is pure.
    effigy_codegraph::docs_context::validate_docs_context_request(query, request)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

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
                    retrieve_documentation_context(&worker_root, &owned_query, request, output_json)
                },
            )
        }
        None => retrieve_documentation_context(repo_root, query, request, output_json),
    }
}

/// Map a refresh verdict to the stderr progress notice.
///
/// `Some` only when the refresh will actually do work — a cold build or a
/// stale rebuild — so warm and current queries never claim one.
pub(super) fn refresh_progress_message(pending: RefreshPending) -> Option<String> {
    match pending {
        RefreshPending::Cold => Some(format!(
            "[docs] {DOCS_CONTEXT_COMMAND}: graph index is missing; building the shared graph index before answering"
        )),
        RefreshPending::Stale => Some(format!(
            "[docs] {DOCS_CONTEXT_COMMAND}: graph index is stale; refreshing it before answering"
        )),
        RefreshPending::Current => None,
    }
}

/// Route one query across the portfolio named by `--sources`.
///
/// The wall-clock bound is applied per repository, not once around the whole
/// walk: a neighbour that cannot answer inside its budget is reported as
/// `timeout` and the next repository is still asked. Repositories are visited
/// sequentially, each through the unchanged single-repository entry point.
fn run_context_sources(
    portfolio: &Path,
    query: &str,
    request: DocsContextRequest,
    only: &[String],
    output_json: bool,
) -> Result<String, RunnerError> {
    let payload = docs_context_sources(portfolio, query, request, only, |repo_root| {
        query_one_repository(repo_root, query, request)
    })
    .map_err(|error| RunnerError::task_invocation(error.to_string()))?;

    let text = render_sources_text(&payload);
    let json = serde_json::to_value(&payload)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    render_command_result(output_json, payload.answered(), json, text)
}

fn query_one_repository(
    repo_root: &Path,
    query: &str,
    request: DocsContextRequest,
) -> SourceQueryOutcome {
    let owned_root = repo_root.to_path_buf();
    let owned_query = query.to_owned();
    let retrieve = move || {
        effigy_codegraph::docs_context(&owned_root, &owned_query, request)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))
    };
    let result = match super::super::graph_time_budget::graph_time_budget() {
        Some(budget) => super::super::graph_time_budget::run_bounded_graph_value(
            repo_root,
            DOCS_CONTEXT_COMMAND,
            budget,
            retrieve,
        ),
        None => retrieve(),
    };
    match result {
        Ok(payload) => SourceQueryOutcome::Answered(Box::new(payload)),
        Err(RunnerError::GraphOperationTimeout { .. }) => SourceQueryOutcome::TimedOut,
        Err(error) => SourceQueryOutcome::Failed(error.to_string()),
    }
}

fn retrieve_documentation_context(
    repo_root: &Path,
    query: &str,
    request: DocsContextRequest,
    output_json: bool,
) -> Result<String, RunnerError> {
    let payload =
        effigy_codegraph::docs_context_with_progress(repo_root, query, request, |pending| {
            if let Some(notice) = refresh_progress_message(pending) {
                eprintln!("{notice}");
            }
        })
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

/// Text output mirrors the JSON grouping: one block per repository, in
/// portfolio order, with its own identity header. There is no combined
/// ranking line, because there is no combined ranking.
fn render_sources_text(payload: &DocsContextSourcesPayload) -> String {
    let mut lines = vec![
        format!("docs context `{}` across sources", payload.query),
        format!("portfolio: {}", payload.portfolio_path),
        format!("directories: {}", payload.directories.join(", ")),
        format!(
            "budgets per repository: sections {}, bytes {}, hops {}",
            payload.budgets.applied.max_sections,
            payload.budgets.applied.max_bytes,
            payload.budgets.applied.max_hops
        ),
        format!("repositories: {}", payload.repositories.len()),
    ];
    if !payload.only.is_empty() {
        lines.push(format!("only: {}", payload.only.join(", ")));
    }
    for repository in &payload.repositories {
        lines.push(String::new());
        lines.extend(render_repository(repository));
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

fn render_repository(repository: &DocsContextRepositoryPayload) -> Vec<String> {
    let mut lines = vec![format!(
        "== {} [{}] {}",
        repository.handle,
        repository.status,
        repository.path.as_deref().unwrap_or("-")
    )];
    lines.push(format!(
        "   head {} indexed {}",
        short_identity(repository.current_head.as_deref()),
        short_identity(repository.indexed_head.as_deref())
    ));
    if let Some(freshness) = &repository.freshness {
        lines.push(format!(
            "   graph: {} - {}{}",
            freshness.state,
            freshness.summary,
            repository
                .profile_state
                .as_deref()
                .map(|state| format!(" (profile {state})"))
                .unwrap_or_default()
        ));
    }
    if !repository.front_doors.is_empty() {
        lines.push(format!(
            "   front doors: {}",
            repository.front_doors.join(", ")
        ));
    }
    if !repository.skill_roots.is_empty() {
        lines.push(format!(
            "   skill roots: {}",
            repository.skill_roots.join(", ")
        ));
    }
    if let Some(next_step) = &repository.next_step {
        lines.push(format!("   next: {next_step}"));
    }
    if repository.results.is_empty() && repository.status == STATUS_OK {
        lines.push("   no section matched in this repository".to_owned());
    }
    for result in &repository.results {
        lines.push(String::new());
        lines.extend(render_source_result(result));
    }
    lines
}

fn render_source_result(result: &DocsContextSourceResultPayload) -> Vec<String> {
    let mut lines = render_result(&result.result);
    // Identity sits with the span it qualifies, so a working-tree excerpt can
    // never be read as committed bytes.
    lines.insert(2, format!("   identity: {}", result.content_identity));
    lines
}

fn short_identity(value: Option<&str>) -> String {
    match value {
        Some(value) => value.chars().take(12).collect(),
        None => "-".to_owned(),
    }
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
