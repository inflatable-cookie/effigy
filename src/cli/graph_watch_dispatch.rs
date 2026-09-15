use std::io::{self, Write};
use std::path::Path;

use effigy_cli::{GraphArgs, GraphSubcommand};
use effigy_codegraph::json::{
    render_json, GraphCatalogPayload, GraphCommandPayload, GraphWatchEventPayload,
};
use effigy_codegraph::scope::{GraphScope, GraphScopePlan, GraphScopeRequest};
use effigy_codegraph::{watch_repo, CodeGraphError, GraphWatchEvent, GraphWatchOptions};
use effigy_ui::{PlainRenderer, Renderer};

use crate::{render_cli_header, CliExecutionContext};

pub fn run_graph_watch_command(context: &CliExecutionContext<'_>, args: GraphArgs) {
    let GraphArgs {
        subcommand: GraphSubcommand::Watch { debounce_ms },
        output_json,
        catalog,
        ..
    } = args
    else {
        unreachable!("graph watch dispatch only handles watch subcommands");
    };

    let mut renderer = PlainRenderer::stdout(context.output_mode);
    if !context.suppress_header {
        let _ = render_cli_header(&mut renderer, context.command_root);
    }

    let repo_root = effigy_routing::owning_workspace_root(context.command_root)
        .unwrap_or_else(|| context.command_root.to_path_buf());
    let repo_root = repo_root.as_path();
    let scope = match resolve_watch_scope(repo_root, catalog.as_deref()) {
        Ok(scope) => scope,
        Err(error) => {
            emit_watch_failure(output_json, repo_root.display().to_string(), error, None);
            std::process::exit(1);
        }
    };
    let catalog_payload = GraphCatalogPayload::from_scope(&scope);
    let options = GraphWatchOptions { debounce_ms };
    let result = watch_repo(&scope, &options, |event| {
        emit_watch_event(
            output_json,
            &mut renderer,
            repo_root.display().to_string(),
            &catalog_payload,
            event,
        )
    });

    if let Err(error) = result {
        emit_watch_failure(
            output_json,
            repo_root.display().to_string(),
            error,
            Some(catalog_payload),
        );
        std::process::exit(1);
    }
}

/// Watch resolves one catalog scope. `--all-catalogs` is rejected by the
/// parser, so fan-out never reaches this point.
fn resolve_watch_scope(
    repo_root: &Path,
    catalog: Option<&str>,
) -> Result<GraphScope, CodeGraphError> {
    let request = match catalog {
        Some(alias) => GraphScopeRequest::Catalog(alias.to_owned()),
        None => GraphScopeRequest::Cwd,
    };
    match effigy_routing::load_effective_catalogs(repo_root) {
        Ok(catalogs) => {
            let cwd = std::env::current_dir().unwrap_or_else(|_| repo_root.to_path_buf());
            match effigy_codegraph::select_scopes(repo_root, &catalogs, &request, &cwd)? {
                GraphScopePlan::Single(scope) => Ok(scope),
                GraphScopePlan::FanOut(_) => Err(CodeGraphError::validation(
                    "`graph watch` watches one catalog at a time",
                )),
            }
        }
        Err(effigy_routing::RoutingError::TaskCatalogsMissing { .. }) => {
            if catalog.is_some() {
                return Err(CodeGraphError::validation(
                    "catalog selection requires an effective catalog root manifest",
                ));
            }
            GraphScope::repo_root(repo_root)
        }
        Err(error) => Err(CodeGraphError::validation(error.to_string())),
    }
}

fn emit_watch_event(
    json_mode: bool,
    renderer: &mut impl Renderer,
    repo_root: String,
    catalog: &GraphCatalogPayload,
    event: GraphWatchEvent,
) -> Result<(), CodeGraphError> {
    if json_mode {
        let payload = GraphCommandPayload::new(
            "effigy.graph.watch.event.v1",
            "graph watch",
            repo_root,
            event.payload,
        )
        .with_catalog(catalog.clone());
        let rendered = render_json(
            &payload,
            "{\"schema\":\"effigy.graph.watch.event.v1\",\"schema_version\":1}",
        );
        let compact = serde_json::from_str::<serde_json::Value>(&rendered)
            .ok()
            .and_then(|value| serde_json::to_string(&value).ok())
            .unwrap_or(rendered);
        let mut stdout = io::stdout().lock();
        writeln!(stdout, "{compact}")
            .and_then(|_| stdout.flush())
            .map_err(|error| {
                CodeGraphError::validation(format!("graph watch emit failed: {error}"))
            })
    } else {
        renderer
            .text(&render_watch_text(&event.payload))
            .map_err(|error| {
                CodeGraphError::validation(format!("graph watch emit failed: {error}"))
            })
    }
}

fn emit_watch_failure(
    json_mode: bool,
    repo_root: String,
    error: CodeGraphError,
    catalog: Option<GraphCatalogPayload>,
) {
    if json_mode {
        let mut payload = GraphCommandPayload::new(
            "effigy.graph.watch.event.v1",
            "graph watch",
            repo_root,
            GraphWatchEventPayload {
                kind: "fatal".to_owned(),
                debounce_ms: 0,
                changed_paths: Vec::new(),
                dirty: false,
                refresh_duration_ms: None,
                index: None,
                notes: vec![error.to_string()],
            },
        );
        if let Some(catalog) = catalog {
            payload = payload.with_catalog(catalog);
        }
        if let Ok(compact) = serde_json::to_string(&payload) {
            println!("{compact}");
            return;
        }
    }
    eprintln!("{error}");
}

fn render_watch_text(payload: &GraphWatchEventPayload) -> String {
    match payload.kind.as_str() {
        "started" => format!("graph watch started\n{}", payload.notes.join("\n")),
        "refresh" => {
            let changed = if payload.changed_paths.is_empty() {
                "0 changed paths".to_owned()
            } else {
                format!("{} changed paths", payload.changed_paths.len())
            };
            let duration = payload
                .refresh_duration_ms
                .map(|value| format!("{value}ms"))
                .unwrap_or_else(|| "unknown".to_owned());
            let mut lines = vec![format!("graph watch refresh: {changed} in {duration}")];
            if let Some(index) = &payload.index {
                lines.push(format!(
                    "indexed: {} files, changed: {}, deleted: {}, failed: {}",
                    index.indexed_files,
                    index.changed_paths.len(),
                    index.deleted_paths.len(),
                    index.failed_paths.len()
                ));
            }
            lines.join("\n")
        }
        "dirty" => format!("graph watch dirty\n{}", payload.notes.join("\n")),
        "reconcile" => {
            let duration = payload
                .refresh_duration_ms
                .map(|value| format!("{value}ms"))
                .unwrap_or_else(|| "unknown".to_owned());
            let mut lines = vec![format!("graph watch reconcile in {duration}")];
            if !payload.notes.is_empty() {
                lines.push(payload.notes.join("\n"));
            }
            if let Some(index) = &payload.index {
                lines.push(format!(
                    "indexed: {} files, changed: {}, deleted: {}, failed: {}",
                    index.indexed_files,
                    index.changed_paths.len(),
                    index.deleted_paths.len(),
                    index.failed_paths.len()
                ));
            }
            lines.join("\n")
        }
        "fatal" => format!("graph watch failed\n{}", payload.notes.join("\n")),
        other => format!("graph watch {other}"),
    }
}
