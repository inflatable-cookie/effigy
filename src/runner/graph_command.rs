use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use effigy_cli::{GraphArgs, GraphSubcommand};
use effigy_codegraph::json::{
    GraphAffectedPayload, GraphCommandPayload, GraphContextPayload, GraphExplorePayload,
    GraphFilesPayload, GraphFreshnessPayload, GraphImpactPayload, GraphIndexPayload,
    GraphNodePayload, GraphRelatedNodesPayload, GraphSearchPayload, GraphStatusPayload,
};
use effigy_codegraph::{
    affected, callees, callers, context, explore, impact, node, query_files, query_search,
    render_json, run_index, status, status_with_refresh,
};

use crate::runner::command_context::resolve_active_repo_root;

use super::error::RunnerError;

/// Default wall-clock budget for a single graph command.
///
/// Graph reads refresh a stale index on demand, so any query can turn into a
/// full repo walk. Unbounded, that walk is indistinguishable from a hang: the
/// caller waits forever with no way to tell "slow first index" from "wedged".
/// Two minutes is well clear of a cold index on a pruned tree and still ends.
const DEFAULT_GRAPH_TIMEOUT_MS: u64 = 120_000;

/// Env override for the budget. `0` disables the bound entirely.
const GRAPH_TIMEOUT_ENV: &str = "EFFIGY_GRAPH_TIMEOUT_MS";

pub(super) fn run_graph(args: GraphArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;
    match graph_time_budget().filter(|_| subcommand_is_bounded(&args.subcommand)) {
        Some(budget) => run_graph_operation_bounded(&repo_root, args, budget),
        None => run_graph_operation(&repo_root, args),
    }
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

/// Resolve the configured budget, or `None` when the bound is switched off.
fn graph_time_budget() -> Option<Duration> {
    let millis = match std::env::var(GRAPH_TIMEOUT_ENV) {
        Ok(raw) => raw
            .trim()
            .parse::<u64>()
            .unwrap_or(DEFAULT_GRAPH_TIMEOUT_MS),
        Err(_) => DEFAULT_GRAPH_TIMEOUT_MS,
    };
    (millis > 0).then(|| Duration::from_millis(millis))
}

/// Run a graph command on a worker thread and give up on it after `budget`.
///
/// The worker is deliberately detached rather than cancelled: graph work runs
/// under a cross-process refresh lock that the OS releases when the process
/// exits, and a half-written index is worse than a slow one. The caller gets a
/// bounded failure carrying the health snapshot; the CLI exits right after.
fn run_graph_operation_bounded(
    repo_root: &Path,
    args: GraphArgs,
    budget: Duration,
) -> Result<String, RunnerError> {
    let label = graph_command_label(&args.subcommand);
    let (sender, receiver) = mpsc::channel();
    let worker_root = repo_root.to_path_buf();
    let worker_args = args.clone();
    let spawned = std::thread::Builder::new()
        .name("effigy-graph".to_owned())
        .spawn(move || {
            let _ = sender.send(run_graph_operation(&worker_root, worker_args));
        });
    if spawned.is_err() {
        // No worker thread available: an unbounded run still beats refusing.
        return run_graph_operation(repo_root, args);
    }
    match receiver.recv_timeout(budget) {
        Ok(result) => result,
        Err(mpsc::RecvTimeoutError::Timeout) => Err(graph_timeout_error(repo_root, label, budget)),
        // The worker died without sending; surface it as a bounded failure too.
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Err(graph_timeout_error(repo_root, label, budget))
        }
    }
}

fn graph_timeout_error(repo_root: &Path, command: &str, budget: Duration) -> RunnerError {
    let timeout_ms = budget.as_millis().min(u128::from(u64::MAX)) as u64;
    let health = effigy_codegraph::health(repo_root);
    let rendered = serde_json::json!({
        "schema": "effigy.graph.timeout.v1",
        "schema_version": 1,
        "command": command,
        "repo_root": repo_root.display().to_string(),
        "timeout_ms": timeout_ms,
        "timeout_env": GRAPH_TIMEOUT_ENV,
        "health": health,
        "next": [
            "run `effigy graph status --json` to inspect index freshness",
            format!("raise the budget with `{GRAPH_TIMEOUT_ENV}=<ms>` (0 disables it)"),
            "run `effigy graph index --json` once to pay the cold build separately",
        ],
    })
    .to_string();
    RunnerError::GraphOperationTimeout {
        command: command.to_owned(),
        timeout_ms,
        rendered,
    }
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

fn run_graph_operation(repo_root: &Path, args: GraphArgs) -> Result<String, RunnerError> {
    let repo_root = repo_root.to_path_buf();
    match args.subcommand {
        GraphSubcommand::Index => {
            let report = run_index(&repo_root).map_err(map_graph_error)?;
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
            if args.output_json {
                Ok(render_json(
                    &GraphCommandPayload::new(
                        "effigy.graph.index.v1",
                        "graph index",
                        repo_root.display().to_string(),
                        payload,
                    ),
                    "{\"schema\":\"effigy.graph.index.v1\",\"schema_version\":1}",
                ))
            } else {
                Ok(render_index_text(&payload))
            }
        }
        GraphSubcommand::Status { refresh } => {
            let payload = if refresh {
                status_with_refresh(&repo_root).map_err(map_graph_error)?
            } else {
                status(&repo_root).map_err(map_graph_error)?
            };
            if args.output_json {
                Ok(render_json(
                    &GraphCommandPayload::new(
                        "effigy.graph.status.v1",
                        "graph status",
                        repo_root.display().to_string(),
                        payload,
                    ),
                    "{\"schema\":\"effigy.graph.status.v1\",\"schema_version\":1}",
                ))
            } else {
                Ok(render_status_text(&payload))
            }
        }
        GraphSubcommand::Watch { .. } => Err(RunnerError::task_invocation(
            "`graph watch` is a streaming command and must run through the CLI entrypoint"
                .to_owned(),
        )),
        GraphSubcommand::Search { query, limit } => {
            let payload = query_search(&repo_root, &query, limit).map_err(map_graph_error)?;
            let text = render_search_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.search.v1",
                "graph search",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Files { limit } => {
            let payload = query_files(&repo_root, limit).map_err(map_graph_error)?;
            let text = render_files_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.files.v1",
                "graph files",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Node { id } => {
            let payload = node(&repo_root, &id).map_err(map_graph_error)?;
            let text = render_node_text(&id, &payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.node.v1",
                "graph node",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Callers { id, limit } => {
            let payload = callers(&repo_root, &id, limit).map_err(map_graph_error)?;
            let text = render_related_text("callers", &payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.callers.v1",
                "graph callers",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Callees { id, limit } => {
            let payload = callees(&repo_root, &id, limit).map_err(map_graph_error)?;
            let text = render_related_text("callees", &payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.callees.v1",
                "graph callees",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Impact { target, limit } => {
            let payload = impact(&repo_root, &target, limit).map_err(map_graph_error)?;
            let text = render_impact_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.impact.v1",
                "graph impact",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Affected {
            changed_paths,
            read_stdin,
            depth,
            limit,
        } => {
            let mut collected = changed_paths;
            if read_stdin {
                collected.extend(read_stdin_paths().map_err(RunnerError::task_invocation)?);
            }
            let payload =
                affected(&repo_root, &collected, depth, limit).map_err(map_graph_error)?;
            let text = render_affected_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.affected.v1",
                "graph affected",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Context {
            request,
            max_files,
            max_bytes,
            languages,
            paths,
        } => {
            let payload = context(
                &repo_root, &request, max_files, max_bytes, &languages, &paths,
            )
            .map_err(map_graph_error)?;
            let text = render_context_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.context.v1",
                "graph context",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
        GraphSubcommand::Explore {
            request,
            max_files,
            max_bytes,
            languages,
            paths,
        } => {
            let payload = explore(
                &repo_root, &request, max_files, max_bytes, &languages, &paths,
            )
            .map_err(map_graph_error)?;
            let text = render_explore_text(&payload);
            render_json_or_text(
                args.output_json,
                "effigy.graph.explore.v1",
                "graph explore",
                repo_root.display().to_string(),
                payload,
                text,
            )
        }
    }
}

fn render_json_or_text<T: serde::Serialize>(
    json_mode: bool,
    schema: &str,
    command: &str,
    repo_root: String,
    payload: T,
    text: String,
) -> Result<String, RunnerError> {
    if json_mode {
        Ok(render_json(
            &GraphCommandPayload::new(schema, command, repo_root, payload),
            "{\"schema\":\"effigy.graph.v1\",\"schema_version\":1}",
        ))
    } else {
        Ok(text)
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
