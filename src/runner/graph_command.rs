use effigy_cli::{GraphArgs, GraphSubcommand};
use effigy_codegraph::json::{
    GraphCommandPayload, GraphContextPayload, GraphExplorePayload, GraphFilesPayload,
    GraphFreshnessPayload, GraphImpactPayload, GraphIndexPayload, GraphNodePayload,
    GraphRelatedNodesPayload, GraphSearchPayload, GraphStatusPayload,
};
use effigy_codegraph::{
    callees, callers, context, explore, impact, node, query_files, query_search, render_json,
    run_index, status,
};

use crate::runner::command_context::resolve_active_repo_root;

use super::error::RunnerError;

pub(super) fn run_graph(args: GraphArgs) -> Result<String, RunnerError> {
    let resolved = resolve_active_repo_root(args.repo_override.clone())?;
    let repo_root = resolved.resolved_root;

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
        GraphSubcommand::Status => {
            let payload = status(&repo_root).map_err(map_graph_error)?;
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
        "graph ready: {}\nfiles: {}\nsymbols: {}\nedges: {}\nstale: {}",
        payload.ready,
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
        "primary: {} excerpts: {} relations: {}",
        payload.primary.len(),
        payload.excerpts.len(),
        payload.relations.len()
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
    for note in &payload.guidance {
        lines.push(format!("- guidance {note}"));
    }
    lines.join("\n")
}

fn freshness_lines(freshness: &GraphFreshnessPayload) -> Vec<String> {
    if freshness.stale {
        vec![format!(
            "graph stale: {} paths require reindex",
            freshness.stale_paths.len()
        )]
    } else {
        Vec::new()
    }
}

fn map_graph_error(error: effigy_codegraph::CodeGraphError) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}
