//! `effigy tasks status`: the task engine's run journal plus a fresh gate
//! probe (contract 018, `effigy.tasks-status.v2`). A pure read: nothing is
//! created, imported or repaired. Kernel run rows are the only task status;
//! v0.14 status records are neither read nor imported (contract 052 §10.1).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_cli::TasksArgs;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::{parse_task_selector, CatalogSelectionMode};
use effigy_ui::{text_renderer, Renderer};
use serde::Serialize;
use serde_json::json;

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use crate::runner::task_engine::{
    live_holders, task_status, HolderKind, RunView, TaskStatusView, TaskTarget, HISTORY_LIMIT,
};
use effigy_routing::select_catalog_and_task;

pub(super) fn run_task_status(args: &TasksArgs, raw_selector: &str) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = context.resolved.resolved_root.clone();
    let catalogs = effigy_routing::load_effective_catalogs_allow_missing(&repo_root)?;
    let selector = parse_task_selector(raw_selector)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let selection = select_catalog_and_task(&selector, &catalogs, &context.invocation_cwd)
        .map_err(RunnerError::from)?;
    let target = TaskTarget::for_selection(&repo_root, &selector.task_name, &selection);
    let view = task_status(&target, HISTORY_LIMIT);

    if args.output_json {
        let payload = json!({
            "schema": "effigy.tasks-status.v2",
            "schema_version": 2,
            "task": task_json(&target),
            "state": view.state,
            "current": view.current,
            "latest": view.latest,
            "history": view.history,
            "next": view.next,
            "warnings": view.warnings,
            "routing": {
                "repo_root": repo_root.display().to_string(),
                "catalog_alias": selection.catalog.alias,
                "catalog_root": selection.catalog.catalog_root.display().to_string(),
                "catalog_manifest_path": selection.catalog.manifest_path.display().to_string(),
                "selection_mode": selection_mode_label(selection.mode),
                "evidence": selection.evidence,
            },
        });
        return render_json(&payload);
    }
    Ok(render_task_status_text(
        &repo_root,
        &target,
        &selection.catalog.alias,
        &selection.evidence,
        &view,
    ))
}

pub(super) fn run_task_status_all(args: &TasksArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = context.resolved.resolved_root;
    let catalogs = effigy_routing::load_effective_catalogs_allow_missing(&repo_root)?;
    let rows = build_status_inventory_rows(&repo_root, &catalogs);

    if args.output_json {
        let mut counts = BTreeMap::<String, usize>::new();
        for row in &rows {
            *counts.entry(row.state.clone()).or_default() += 1;
        }
        let catalog_scopes = catalogs
            .iter()
            .map(|catalog| {
                json!({
                    "alias": catalog.alias,
                    "root": catalog.catalog_root.display().to_string(),
                    "manifest": catalog.manifest_path.display().to_string(),
                    "depth": catalog.depth,
                })
            })
            .collect::<Vec<_>>();
        return render_json(&json!({
            "schema": "effigy.tasks-status-all.v2",
            "schema_version": 2,
            "scope_root": repo_root.display().to_string(),
            "catalog_scopes": catalog_scopes,
            "counts_by_state": counts,
            "warnings": [],
            "rows": rows,
        }));
    }
    Ok(render_task_status_all_text(&repo_root, &rows))
}

#[derive(Debug, Clone, Serialize)]
struct TaskStatusInventoryRow {
    selector: String,
    selected_catalog_root: String,
    state: String,
    currently_declared: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    current: Option<RunView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest: Option<RunView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    next: Option<String>,
    warnings: Vec<String>,
}

fn build_status_inventory_rows(
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<TaskStatusInventoryRow> {
    let mut rows = Vec::new();
    let mut declared = BTreeSet::new();
    for catalog in catalogs {
        for task_name in catalog.manifest.tasks.keys() {
            let target = TaskTarget::for_catalog(repo_root, task_name, catalog);
            declared.insert((target.catalog.clone(), target.task.clone()));
            let view = task_status(&target, 2);
            rows.push(row(repo_root, &target, &catalog.catalog_root, true, view));
        }
    }
    // A live run of a task no longer declared still holds its gate.
    for holder in live_holders(repo_root) {
        if let HolderKind::Unreadable(reason) = &holder.kind {
            rows.push(TaskStatusInventoryRow {
                selector: "*".to_owned(),
                selected_catalog_root: "root".to_owned(),
                state: "unknown".to_owned(),
                currently_declared: false,
                current: None,
                latest: None,
                next: None,
                warnings: vec![format!("run journal is unreadable: {reason}")],
            });
            continue;
        }
        if holder.kind != HolderKind::Task
            || declared.contains(&(holder.catalog.clone(), holder.task.clone()))
        {
            continue;
        }
        let label = if holder.catalog.is_empty() {
            holder.task.clone()
        } else {
            format!("{}/{}", holder.catalog, holder.task)
        };
        rows.push(TaskStatusInventoryRow {
            selector: label,
            selected_catalog_root: if holder.catalog.is_empty() {
                "root".to_owned()
            } else {
                holder.catalog.clone()
            },
            state: "running".to_owned(),
            currently_declared: false,
            current: None,
            latest: None,
            next: None,
            warnings: vec![format!(
                "run {} of an undeclared task still holds its gate (pid {})",
                holder.run.map(|run| run.to_string()).unwrap_or_default(),
                holder
                    .pid
                    .map(|pid| pid.to_string())
                    .unwrap_or_else(|| "unreadable".to_owned())
            )],
        });
    }
    rows.sort_by(|left, right| {
        left.selected_catalog_root
            .cmp(&right.selected_catalog_root)
            .then_with(|| left.selector.cmp(&right.selector))
    });
    rows
}

fn row(
    repo_root: &Path,
    target: &TaskTarget,
    catalog_root: &Path,
    currently_declared: bool,
    view: TaskStatusView,
) -> TaskStatusInventoryRow {
    TaskStatusInventoryRow {
        selector: target.label.clone(),
        selected_catalog_root: relative_or_absolute(repo_root, catalog_root),
        state: view.state,
        currently_declared,
        current: view.current,
        latest: view.latest,
        next: view.next,
        warnings: view.warnings,
    }
}

fn task_json(target: &TaskTarget) -> serde_json::Value {
    json!({
        "selector": target.label,
        "catalog": target.catalog,
        "name": target.task,
        "surface": match target.surface {
            effigy_core::task_selection::TaskSurface::Published => "published",
            effigy_core::task_selection::TaskSurface::Draft => "draft",
        },
    })
}

fn render_json(payload: &serde_json::Value) -> Result<String, RunnerError> {
    serde_json::to_string_pretty(payload).map_err(|error| {
        RunnerError::task_invocation(format!("failed to render task status json: {error}"))
    })
}

fn render_task_status_text(
    repo_root: &Path,
    target: &TaskTarget,
    catalog_alias: &str,
    evidence: &[String],
    view: &TaskStatusView,
) -> String {
    use effigy_core::widgets::KeyValue;
    let mut renderer = text_renderer();
    let _ = renderer.section("Task Status");
    let _ = renderer.key_values(&[
        KeyValue::new("task", target.label.clone()),
        KeyValue::new("state", view.state.clone()),
        KeyValue::new("catalog", catalog_alias.to_owned()),
        KeyValue::new("repo-root", repo_root.display().to_string()),
    ]);
    if let Some(current) = &view.current {
        let _ = renderer.text("");
        let mut values = vec![
            KeyValue::new("run", current.run_id.clone()),
            KeyValue::new("phase", current.phase.to_owned()),
            KeyValue::new("started", epoch_text(current.started_at_epoch_ms)),
        ];
        if let Some(pid) = current.pid {
            values.push(KeyValue::new("pid", pid.to_string()));
        }
        if let Some(profile) = &current.profile {
            values.push(KeyValue::new("profile", profile.clone()));
        }
        let _ = renderer.key_values(&values);
    }
    if let Some(next) = &view.next {
        let _ = renderer.text("");
        let _ = renderer.text(&format!(
            "The run's owner exited without a result. `{next}` settles it before starting again."
        ));
    }
    if let Some(latest) = &view.latest {
        let _ = renderer.text("");
        let mut values = vec![
            KeyValue::new("last-run", latest.run_id.clone()),
            KeyValue::new("outcome", latest.state.clone()),
            KeyValue::new("started", epoch_text(latest.started_at_epoch_ms)),
        ];
        if let Some(code) = latest.exit_code {
            values.push(KeyValue::new("exit-code", code.to_string()));
        }
        if let Some(duration) = latest.duration_ms {
            values.push(KeyValue::new("duration-ms", duration.to_string()));
        }
        if let Some(cleanup) = latest.cleanup {
            values.push(KeyValue::new("cleanup", cleanup.to_owned()));
        }
        let _ = renderer.key_values(&values);
    }
    for (label, run) in [
        ("current steps", view.current.as_ref()),
        ("last-run steps", view.latest.as_ref()),
    ] {
        if let Some(run) = run {
            let mut lines = Vec::new();
            step_status_lines(&run.steps, "", &mut lines);
            if !lines.is_empty() {
                let _ = renderer.bullet_list(label, &lines);
            }
        }
    }
    if view.history.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.text("No runs recorded yet.");
    }
    if !view.warnings.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("warnings", &view.warnings);
    }
    if !evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("routing-evidence", evidence);
    }
    String::from_utf8_lossy(&renderer.into_inner()).to_string()
}

fn step_status_lines(steps: &[RunView], prefix: &str, lines: &mut Vec<String>) {
    for step in steps {
        let label = format!("{prefix}{}", step.step.as_deref().unwrap_or("step"));
        lines.push(format!(
            "{label}: {} (exit {}, cleanup {}, run {})",
            step.state,
            step.exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "pending".to_owned()),
            step.cleanup.unwrap_or("pending"),
            step.run_id
        ));
        step_status_lines(&step.steps, &format!("{label}/"), lines);
    }
}

fn render_task_status_all_text(repo_root: &Path, rows: &[TaskStatusInventoryRow]) -> String {
    let mut renderer = text_renderer();
    let _ = renderer.section("Task Status");
    let _ = renderer.key_values(&[
        effigy_core::widgets::KeyValue::new("scope-root", repo_root.display().to_string()),
        effigy_core::widgets::KeyValue::new("row-count", rows.len().to_string()),
    ]);
    let mut grouped = BTreeMap::<String, Vec<&TaskStatusInventoryRow>>::new();
    for row in rows {
        grouped
            .entry(row.selected_catalog_root.clone())
            .or_default()
            .push(row);
    }
    for (catalog_root, entries) in grouped {
        let _ = renderer.text("");
        let _ = renderer.section(&format!("Catalog: {catalog_root}"));
        for row in entries {
            let mut line = format!("- {} [{}]", row.selector, row.state);
            if !row.currently_declared {
                line.push_str(" (no-longer-declared)");
            }
            let when = row
                .current
                .as_ref()
                .or(row.latest.as_ref())
                .map(|run| epoch_text(run.started_at_epoch_ms));
            if let Some(when) = when {
                line.push_str(&format!(" started={when}"));
            }
            let _ = renderer.text(&line);
            for warning in &row.warnings {
                let _ = renderer.text(&format!("  warning: {warning}"));
            }
        }
    }
    String::from_utf8_lossy(&renderer.into_inner()).to_string()
}

fn epoch_text(epoch_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(epoch_ms)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_else(|| epoch_ms.to_string())
}

fn relative_or_absolute(repo_root: &Path, path: &Path) -> String {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let root: PathBuf = canonical(repo_root);
    let path = canonical(path);
    path.strip_prefix(&root)
        .map(|relative| {
            let rendered = relative.display().to_string();
            if rendered.is_empty() {
                "root".to_owned()
            } else {
                rendered
            }
        })
        .unwrap_or_else(|_| path.display().to_string())
}

fn selection_mode_label(mode: CatalogSelectionMode) -> &'static str {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit-prefix",
        CatalogSelectionMode::CwdNearest => "cwd-nearest",
        CatalogSelectionMode::RootShallowest => "root-shallowest",
    }
}
