use std::path::Path;

use effigy_cli::{GraphArgs, GraphSubcommand};
use effigy_codegraph::json::{
    GraphAffectedPayload, GraphCatalogOutcomePayload, GraphCatalogPayload, GraphCommandPayload,
    GraphContextPayload, GraphExplorePayload, GraphFanOutPayload, GraphFilesPayload,
    GraphFreshnessPayload, GraphImpactPayload, GraphIndexPayload, GraphNodePayload,
    GraphRelatedNodesPayload, GraphSearchPayload, GraphStatusPayload,
};
use effigy_codegraph::scope::{GraphScope, GraphScopePlan, GraphScopeRequest};
use effigy_codegraph::{
    affected_in_scope, callees_in_scope, callers_in_scope, context_in_scope, explore_in_scope,
    files_in_scope, impact_in_scope, node_in_scope, render_json, run_index_in_scope,
    search_in_scope, status_in_scope,
};

use crate::runner::command_context::resolve_active_command_context;

use super::error::RunnerError;

pub(super) fn run_graph(args: GraphArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let invocation_cwd = context.invocation_cwd;
    // When the invocation resolved to a catalog member rather than the
    // workspace that declares it, use the owning workspace so `[catalog.graph]`
    // posture and cwd selection apply.
    let repo_root = effigy_routing::owning_workspace_root(&context.resolved.resolved_root)
        .unwrap_or(context.resolved.resolved_root);
    let args = prepare_args(args)?;
    match super::graph_time_budget::graph_time_budget()
        .filter(|_| subcommand_is_bounded(&args.subcommand))
    {
        Some(budget) => {
            let command = graph_command_label(&args.subcommand);
            let worker_root = repo_root.clone();
            let worker_args = args.clone();
            let worker_cwd = invocation_cwd.clone();
            super::graph_time_budget::run_bounded_graph_operation(
                &repo_root,
                command,
                budget,
                move || run_graph_scoped(&worker_root, &worker_args, &worker_cwd),
            )
        }
        None => run_graph_scoped(&repo_root, &args, &invocation_cwd),
    }
}

/// Read `--stdin` changed paths once, before any scope runs.
///
/// Fan-out runs the same subcommand once per catalog; reading stdin inside the
/// per-scope operation would leave every scope after the first with an empty
/// stream and silently change its input.
fn prepare_args(mut args: GraphArgs) -> Result<GraphArgs, RunnerError> {
    if let GraphSubcommand::Affected {
        changed_paths,
        read_stdin,
        ..
    } = &mut args.subcommand
    {
        if *read_stdin {
            changed_paths.extend(read_stdin_paths().map_err(RunnerError::task_invocation)?);
            *read_stdin = false;
        }
    }
    Ok(args)
}

/// Whether a subcommand runs under the time budget.
///
/// Queries do: they refresh a stale index behind the caller's back, so a slow
/// walk shows up as an unexplained hang. `graph index` and `graph watch` are
/// exempt — the caller explicitly asked for the long-running build.
fn subcommand_is_bounded(subcommand: &GraphSubcommand) -> bool {
    !matches!(
        subcommand,
        GraphSubcommand::Index | GraphSubcommand::Watch { .. }
    )
}

fn graph_command_label(subcommand: &GraphSubcommand) -> &'static str {
    match subcommand {
        GraphSubcommand::Index => "graph index",
        GraphSubcommand::Status { .. } => "graph status",
        GraphSubcommand::Watch { .. } => "graph watch",
        GraphSubcommand::Search { .. } => "graph search",
        GraphSubcommand::Files { .. } => "graph files",
        GraphSubcommand::Node { .. } => "graph node",
        GraphSubcommand::Callers { .. } => "graph callers",
        GraphSubcommand::Callees { .. } => "graph callees",
        GraphSubcommand::Impact { .. } => "graph impact",
        GraphSubcommand::Affected { .. } => "graph affected",
        GraphSubcommand::Context { .. } => "graph context",
        GraphSubcommand::Explore { .. } => "graph explore",
    }
}

fn graph_schema(subcommand: &GraphSubcommand) -> &'static str {
    match subcommand {
        GraphSubcommand::Index => "effigy.graph.index.v1",
        GraphSubcommand::Status { .. } => "effigy.graph.status.v1",
        GraphSubcommand::Watch { .. } => "effigy.graph.watch.event.v1",
        GraphSubcommand::Search { .. } => "effigy.graph.search.v1",
        GraphSubcommand::Files { .. } => "effigy.graph.files.v1",
        GraphSubcommand::Node { .. } => "effigy.graph.node.v1",
        GraphSubcommand::Callers { .. } => "effigy.graph.callers.v1",
        GraphSubcommand::Callees { .. } => "effigy.graph.callees.v1",
        GraphSubcommand::Impact { .. } => "effigy.graph.impact.v1",
        GraphSubcommand::Affected { .. } => "effigy.graph.affected.v1",
        GraphSubcommand::Context { .. } => "effigy.graph.context.v1",
        GraphSubcommand::Explore { .. } => "effigy.graph.explore.v1",
    }
}

/// Resolve the catalog scope plan for one invocation.
///
/// Explicit selection failures fail closed before any refresh. A repository
/// with no effective catalog membership keeps the repository-owned scope, so
/// graph commands still work outside Effigy monorepos.
fn resolve_scope_plan(
    repo_root: &Path,
    args: &GraphArgs,
    invocation_cwd: &Path,
) -> Result<GraphScopePlan, RunnerError> {
    let request = if args.all_catalogs {
        GraphScopeRequest::AllCatalogs
    } else if let Some(alias) = args.catalog.as_ref() {
        GraphScopeRequest::Catalog(alias.clone())
    } else {
        GraphScopeRequest::Cwd
    };
    match effigy_routing::load_effective_catalogs(repo_root) {
        Ok(catalogs) => {
            effigy_codegraph::select_scopes(repo_root, &catalogs, &request, invocation_cwd)
                .map_err(map_graph_error)
        }
        Err(effigy_routing::RoutingError::TaskCatalogsMissing { .. }) => {
            if args.catalog.is_some() || args.all_catalogs {
                return Err(map_graph_error(
                    effigy_codegraph::CodeGraphError::validation(
                        "catalog selection requires an effective catalog root manifest",
                    ),
                ));
            }
            GraphScope::repo_root(repo_root)
                .map(GraphScopePlan::Single)
                .map_err(map_graph_error)
        }
        Err(error) => Err(map_graph_error(
            effigy_codegraph::CodeGraphError::validation(error.to_string()),
        )),
    }
}

fn run_graph_scoped(
    repo_root: &Path,
    args: &GraphArgs,
    invocation_cwd: &Path,
) -> Result<String, RunnerError> {
    let plan = resolve_scope_plan(repo_root, args, invocation_cwd)?;
    match plan {
        GraphScopePlan::Single(scope) => {
            let output = run_scope_operation(args, &scope)?;
            Ok(render_single(repo_root, args, &scope, output))
        }
        GraphScopePlan::FanOut(scopes) => run_fan_out(repo_root, args, &scopes),
    }
}

struct ScopeOutput {
    text: String,
    payload: serde_json::Value,
}

fn run_scope_operation(args: &GraphArgs, scope: &GraphScope) -> Result<ScopeOutput, RunnerError> {
    match &args.subcommand {
        GraphSubcommand::Index => {
            let report = run_index_in_scope(scope).map_err(map_graph_error)?;
            let payload = GraphIndexPayload {
                indexed_files: report.indexed_files,
                extractor_count: report.extractor_count,
                counts: report.counts,
                stale_paths: report.stale_paths,
                new_paths: report.new_paths,
                changed_paths: report.changed_paths,
                deleted_paths: report.deleted_paths,
                skipped_paths: report.skipped_paths,
                failed_paths: report.failed_paths,
            };
            let text = render_index_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Status { refresh } => {
            let payload = status_in_scope(scope, *refresh).map_err(map_graph_error)?;
            let text = render_status_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Watch { .. } => Err(RunnerError::task_invocation(
            "`graph watch` is a streaming command and must run through the CLI entrypoint"
                .to_owned(),
        )),
        GraphSubcommand::Search { query, limit } => {
            let payload = search_in_scope(scope, query, *limit).map_err(map_graph_error)?;
            let text = render_search_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Files { limit } => {
            let payload = files_in_scope(scope, *limit).map_err(map_graph_error)?;
            let text = render_files_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Node { id } => {
            let payload = node_in_scope(scope, id).map_err(map_graph_error)?;
            let text = render_node_text(id, &payload);
            into_output(payload, text)
        }
        GraphSubcommand::Callers { id, limit } => {
            let payload = callers_in_scope(scope, id, *limit).map_err(map_graph_error)?;
            let text = render_related_text("callers", &payload);
            into_output(payload, text)
        }
        GraphSubcommand::Callees { id, limit } => {
            let payload = callees_in_scope(scope, id, *limit).map_err(map_graph_error)?;
            let text = render_related_text("callees", &payload);
            into_output(payload, text)
        }
        GraphSubcommand::Impact { target, limit } => {
            let payload = impact_in_scope(scope, target, *limit).map_err(map_graph_error)?;
            let text = render_impact_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Affected {
            changed_paths,
            depth,
            limit,
            ..
        } => {
            let payload =
                affected_in_scope(scope, changed_paths, *depth, *limit).map_err(map_graph_error)?;
            let text = render_affected_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Context {
            request,
            max_files,
            max_bytes,
            languages,
            paths,
        } => {
            let payload =
                context_in_scope(scope, request, *max_files, *max_bytes, languages, paths)
                    .map_err(map_graph_error)?;
            let text = render_context_text(&payload);
            into_output(payload, text)
        }
        GraphSubcommand::Explore {
            request,
            max_files,
            max_bytes,
            languages,
            paths,
        } => {
            let payload =
                explore_in_scope(scope, request, *max_files, *max_bytes, languages, paths)
                    .map_err(map_graph_error)?;
            let text = render_explore_text(&payload);
            into_output(payload, text)
        }
    }
}

fn into_output(payload: impl serde::Serialize, text: String) -> Result<ScopeOutput, RunnerError> {
    let payload = serde_json::to_value(payload).map_err(|error| {
        RunnerError::task_invocation(format!("failed to render graph payload: {error}"))
    })?;
    Ok(ScopeOutput { text, payload })
}

fn render_single(
    repo_root: &Path,
    args: &GraphArgs,
    scope: &GraphScope,
    output: ScopeOutput,
) -> String {
    if !args.output_json {
        return output.text;
    }
    let schema = graph_schema(&args.subcommand);
    let envelope = GraphCommandPayload::new(
        schema,
        graph_command_label(&args.subcommand),
        repo_root.display().to_string(),
        output.payload,
    )
    .with_catalog(GraphCatalogPayload::from_scope(scope));
    render_json(
        &envelope,
        &format!("{{\"schema\":\"{schema}\",\"schema_version\":1}}"),
    )
}

/// Explicit fan-out: every scope runs separately and reports its own outcome.
///
/// One catalog's failure is reported as a failed catalog instead of being
/// folded into a success, and the command exits non-zero.
fn run_fan_out(
    repo_root: &Path,
    args: &GraphArgs,
    scopes: &[GraphScope],
) -> Result<String, RunnerError> {
    let mut outcomes = Vec::with_capacity(scopes.len());
    let mut text_lines = Vec::new();
    let mut failed = false;
    for scope in scopes {
        let catalog = GraphCatalogPayload::from_scope(scope);
        match run_scope_operation(args, scope) {
            Ok(output) => {
                text_lines.push(format!("catalog {} ({}): ok", catalog.alias, catalog.root));
                text_lines.extend(output.text.lines().map(|line| format!("  {line}")));
                outcomes.push(GraphCatalogOutcomePayload {
                    catalog,
                    ok: true,
                    error: None,
                    payload: Some(output.payload),
                });
            }
            Err(error) => {
                failed = true;
                let message = error.to_string();
                text_lines.push(format!(
                    "catalog {} ({}): failed: {message}",
                    catalog.alias, catalog.root
                ));
                outcomes.push(GraphCatalogOutcomePayload {
                    catalog,
                    ok: false,
                    error: Some(message),
                    payload: None,
                });
            }
        }
    }
    let rendered = if args.output_json {
        let envelope = GraphCommandPayload::new(
            "effigy.graph.fanout.v1",
            graph_command_label(&args.subcommand),
            repo_root.display().to_string(),
            GraphFanOutPayload { catalogs: outcomes },
        );
        render_json(
            &envelope,
            "{\"schema\":\"effigy.graph.fanout.v1\",\"schema_version\":1}",
        )
    } else {
        text_lines.join("\n")
    };
    if failed {
        Err(RunnerError::task_invocation(rendered))
    } else {
        Ok(rendered)
    }
}

fn render_index_text(payload: &GraphIndexPayload) -> String {
    format!(
        "graph indexed {} files\nsymbols: {}\nedges: {}\nstale: {}\nnew: {}\nchanged: {}\ndeleted: {}\nfailed: {}",
        payload.indexed_files,
        payload.counts.symbols,
        payload.counts.edges,
        payload.stale_paths.len(),
        payload.new_paths.len(),
        payload.changed_paths.len(),
        payload.deleted_paths.len(),
        payload.failed_paths.len()
    )
}

fn render_status_text(payload: &GraphStatusPayload) -> String {
    format!(
        "graph ready: {}\ntrust: {}\ntrust summary: {}\nfiles: {}\nsymbols: {}\nedges: {}\nstale: {}",
        payload.ready,
        payload.freshness.state,
        payload.freshness.summary,
        payload.counts.files,
        payload.counts.symbols,
        payload.counts.edges,
        payload.stale_paths.len()
    )
}

fn render_files_text(payload: &GraphFilesPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!("graph files: {}", payload.files.len()));
    for file in payload.files.iter().take(20) {
        lines.push(format!(
            "- {} [{}] {} bytes",
            file.path, file.language_id, file.byte_size
        ));
    }
    lines.join("\n")
}

fn render_search_text(payload: &GraphSearchPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!(
        "graph search `{}`: {} matches",
        payload.query,
        payload.matches.len()
    ));
    for entry in payload.matches.iter().take(20) {
        let label = entry
            .name
            .as_deref()
            .or(entry.path.as_deref())
            .unwrap_or(entry.record_id.as_str());
        lines.push(format!(
            "- {} {} ({})",
            entry.record_type, label, entry.record_id
        ));
    }
    lines.join("\n")
}

fn render_node_text(id: &str, payload: &GraphNodePayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!("graph node `{id}`"));
    if let Some(symbol) = &payload.symbol {
        lines.push(format!(
            "symbol: {} [{}] {}",
            symbol.display_name, symbol.kind, symbol.provenance.source_path
        ));
    }
    if let Some(file) = &payload.file {
        lines.push(format!("file: {} [{}]", file.path, file.language_id));
    }
    lines.push(format!(
        "edges: {} references: {} diagnostics: {}",
        payload.edges.len(),
        payload.references.len(),
        payload.diagnostics.len()
    ));
    lines.join("\n")
}

fn render_related_text(label: &str, payload: &GraphRelatedNodesPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!(
        "graph {label} `{}`: {} nodes, {} edges",
        payload.target_id,
        payload.nodes.len(),
        payload.edges.len()
    ));
    for symbol in payload.nodes.iter().take(20) {
        lines.push(format!(
            "- {} [{}] {}",
            symbol.display_name, symbol.kind, symbol.provenance.source_path
        ));
    }
    lines.join("\n")
}

fn render_impact_text(payload: &GraphImpactPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!(
        "graph impact `{}`: {} files, {} symbols, {} edges",
        payload.target,
        payload.files.len(),
        payload.symbols.len(),
        payload.edges.len()
    ));
    for file in payload.files.iter().take(10) {
        lines.push(format!("- file {}", file.path));
    }
    for symbol in payload.symbols.iter().take(10) {
        lines.push(format!(
            "- symbol {} [{}] {}",
            symbol.display_name, symbol.kind, symbol.provenance.source_path
        ));
    }
    lines.join("\n")
}

fn render_affected_text(payload: &GraphAffectedPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!(
        "graph affected: {} changed, {} affected files, {} likely test files, {} likely test tasks",
        payload.changed_paths.len(),
        payload.affected_files.len(),
        payload.likely_test_files.len(),
        payload.likely_test_tasks.len()
    ));
    lines.push(format!("depth: {}", payload.depth));
    for path in &payload.changed_paths {
        lines.push(format!("- changed {path}"));
    }
    for file in payload.likely_test_files.iter().take(10) {
        lines.push(format!(
            "- test-file {} [{}] because {}",
            file.path,
            file.confidence,
            file.reasons.join("; ")
        ));
    }
    for task in payload.likely_test_tasks.iter().take(10) {
        lines.push(format!(
            "- test-task {} [{}] {} because {}",
            task.name,
            task.kind,
            task.confidence,
            task.reasons.join("; ")
        ));
    }
    for note in &payload.notes {
        lines.push(format!("- note {note}"));
    }
    lines.join("\n")
}

fn render_context_text(payload: &GraphContextPayload) -> String {
    let mut lines = freshness_lines(&payload.freshness);
    lines.push(format!(
        "graph context `{}`: {} items",
        payload.request,
        payload.items.len()
    ));
    lines.push(format!(
        "overflow: {} omitted items, {} omitted files, {} omitted symbols, {} omitted docs, {} / {} bytes",
        payload.overflow.omitted_items,
        payload.overflow.omitted_files,
        payload.overflow.omitted_symbols,
        payload.overflow.omitted_docs,
        payload.overflow.used_bytes,
        payload.overflow.byte_budget
    ));
    for item in payload.items.iter().take(10) {
        let name = item.name.as_deref().unwrap_or(item.path.as_str());
        let language = item.language_id.as_deref().unwrap_or("unknown");
        let reasons = if item.reasons.is_empty() {
            "no reason recorded".to_owned()
        } else {
            item.reasons.join("; ")
        };
        lines.push(format!(
            "- rank {} {} {} [{}] score {} because {}",
            item.rank, item.kind, name, language, item.score, reasons
        ));
        if let Some(snippet) = &item.snippet {
            let suffix = if item.snippet_truncated {
                " (truncated)"
            } else {
                ""
            };
            lines.push(format!("  snippet: {}{}", snippet, suffix));
        }
    }
    for note in &payload.notes {
        lines.push(format!("- note {note}"));
    }
    lines.join("\n")
}

fn render_explore_text(payload: &GraphExplorePayload) -> String {
    let mut lines = freshness_lines(&payload.index.freshness);
    lines.push(format!("graph explore `{}`", payload.query));
    lines.push(payload.summary.clone());
    lines.push(format!(
        "primary: {} excerpts: {} relations: {} edit-targets: {} likely-tests: {} files / {} tasks",
        payload.primary.len(),
        payload.excerpts.len(),
        payload.relations.len(),
        payload.edit_targets.len(),
        payload.likely_test_files.len(),
        payload.likely_test_tasks.len()
    ));
    for item in payload.primary.iter().take(10) {
        let name = item.name.as_deref().unwrap_or(item.path.as_str());
        let reasons = if item.reasons.is_empty() {
            "no reason recorded".to_owned()
        } else {
            item.reasons.join("; ")
        };
        lines.push(format!(
            "- primary rank {} {} score {} because {}",
            item.rank, name, item.score, reasons
        ));
    }
    for excerpt in payload.excerpts.iter().take(10) {
        let name = excerpt.name.as_deref().unwrap_or(excerpt.path.as_str());
        let suffix = if excerpt.truncated {
            " (truncated)"
        } else {
            ""
        };
        lines.push(format!(
            "- excerpt {} [{}] score {}{}",
            name, excerpt.role, excerpt.score, suffix
        ));
        lines.push(format!("  {}", excerpt.text));
    }
    for relation in payload.relations.iter().take(10) {
        let name = relation.name.as_deref().unwrap_or(relation.path.as_str());
        lines.push(format!(
            "- relation {} {} because {}",
            relation.kind, name, relation.reason
        ));
    }
    for target in payload.edit_targets.iter().take(5) {
        let reasons = if target.reasons.is_empty() {
            "no reason recorded".to_owned()
        } else {
            target.reasons.join("; ")
        };
        lines.push(format!(
            "- edit target {} {} [{}] because {}",
            target.kind, target.path, target.confidence, reasons
        ));
    }
    for file in payload.likely_test_files.iter().take(5) {
        let reasons = if file.reasons.is_empty() {
            "no reason recorded".to_owned()
        } else {
            file.reasons.join("; ")
        };
        lines.push(format!(
            "- likely test file {} [{}] because {}",
            file.path, file.confidence, reasons
        ));
    }
    for task in payload.likely_test_tasks.iter().take(5) {
        let reasons = if task.reasons.is_empty() {
            "no reason recorded".to_owned()
        } else {
            task.reasons.join("; ")
        };
        lines.push(format!(
            "- likely test task {} [{}] because {}",
            task.name, task.confidence, reasons
        ));
    }
    for note in &payload.guidance {
        lines.push(format!("- guidance {note}"));
    }
    lines.join("\n")
}

fn freshness_lines(freshness: &GraphFreshnessPayload) -> Vec<String> {
    let mut lines = vec![format!("graph trust: {}", freshness.state)];
    lines.push(format!("graph trust summary: {}", freshness.summary));
    if freshness.stale {
        lines.push(format!(
            "graph stale: {} paths require reindex",
            freshness.stale_path_count
        ));
    }
    if freshness.failed_path_count > 0 {
        lines.push(format!(
            "graph failures: {} paths failed during indexing",
            freshness.failed_path_count
        ));
    }
    lines
}

fn read_stdin_paths() -> Result<Vec<String>, String> {
    use std::io::Read;

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("failed to read stdin for `graph affected`: {error}"))?;
    Ok(input
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect())
}

fn map_graph_error(error: effigy_codegraph::CodeGraphError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}
