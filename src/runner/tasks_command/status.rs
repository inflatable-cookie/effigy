use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use effigy_cli::TasksArgs;
use effigy_execution::{
    ExecutionSelectionCatalogSummary, ExecutionSelectionInput, ExecutionSelectionPlan,
    TaskStatusActiveRecord, TaskStatusCompletedRecord, TaskStatusState, TaskStatusTargetIdentity,
    TaskStatusWarning,
};
use effigy_manifest::LoadedCatalog;
use effigy_runtime::task_status::{
    list_task_status_keys, reconcile_task_status_records, TaskStatusReadSnapshot,
};
use effigy_tasks::{parse_task_selector, render_task_selector, CatalogSelectionMode};
use effigy_ui::{text_renderer, Renderer};

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use effigy_routing::select_catalog_and_task;
use serde::Serialize;
use serde_json::json;

pub(super) fn run_task_status(args: &TasksArgs, raw_selector: &str) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let catalogs =
        effigy_routing::load_effective_catalogs_allow_missing(&context.resolved.resolved_root)?;
    let selector = parse_task_selector(raw_selector)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let selection = select_catalog_and_task(&selector, &catalogs, &context.invocation_cwd)
        .map_err(RunnerError::from)?;
    let resolved_selector = render_task_selector(&selector);
    let plan = ExecutionSelectionPlan::new(
        ExecutionSelectionInput {
            selector: selector.clone(),
            invocation_cwd: context.invocation_cwd.clone(),
            resolved_root: context.resolved.resolved_root.clone(),
        },
        ExecutionSelectionCatalogSummary {
            alias: selection.catalog.alias.clone(),
            catalog_root: selection.catalog.catalog_root.clone(),
            manifest_path: selection.catalog.manifest_path.clone(),
            depth: selection.catalog.depth,
        },
        selection.mode,
        selection.evidence.clone(),
        selector.task_name.clone(),
    );
    let identity = TaskStatusTargetIdentity::new(
        context.resolved.resolved_root.clone(),
        plan.catalog.catalog_root.clone(),
        resolved_selector.clone(),
        plan.task_name.clone(),
        None,
    );
    let snapshot =
        reconcile_task_status_records(&context.resolved.resolved_root, &identity.status_key())
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    let state = effective_state(&snapshot);

    if args.output_json {
        render_task_status_json(
            &context.resolved.resolved_root,
            &plan,
            &resolved_selector,
            state,
            &snapshot,
        )
    } else {
        Ok(render_task_status_text(
            &context.resolved.resolved_root,
            &plan,
            &resolved_selector,
            state,
            &snapshot,
        ))
    }
}

pub(super) fn run_task_status_all(args: &TasksArgs) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let repo_root = context.resolved.resolved_root;
    let catalogs = effigy_routing::load_effective_catalogs_allow_missing(&repo_root)?;
    let rows = build_status_inventory_rows(&repo_root, &catalogs)?;

    if args.output_json {
        render_task_status_all_json(&repo_root, &catalogs, &rows)
    } else {
        Ok(render_task_status_all_text(&repo_root, &rows))
    }
}

#[derive(Debug, Clone, Serialize)]
struct TaskStatusInventoryRow {
    selector: String,
    selected_catalog_root: String,
    state: String,
    currently_declared: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    no_longer_declared: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    last_updated: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    route: Option<String>,
    active: Option<TaskStatusActiveRecord>,
    latest: Option<TaskStatusCompletedRecord>,
    stale_active: Option<TaskStatusActiveRecord>,
    warnings: Vec<TaskStatusWarning>,
}

#[derive(Debug, Clone)]
struct DeclaredTaskStatusTarget {
    identity: TaskStatusTargetIdentity,
    selector: String,
    selected_catalog_root: PathBuf,
}

fn build_status_inventory_rows(
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Vec<TaskStatusInventoryRow>, RunnerError> {
    let declared = declared_task_targets(repo_root, catalogs);
    let mut rows = Vec::new();
    let mut seen = BTreeSet::new();

    for target in declared.values() {
        let key = target.identity.status_key();
        let snapshot = reconcile_task_status_records(repo_root, &key)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        seen.insert(key.as_str().to_owned());
        rows.push(build_inventory_row(
            repo_root,
            target.selector.clone(),
            target.selected_catalog_root.clone(),
            true,
            false,
            snapshot,
        ));
    }

    for key in list_task_status_keys(repo_root)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    {
        if seen.contains(key.as_str()) {
            continue;
        }
        let snapshot = reconcile_task_status_records(repo_root, &key)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
        let Some(identity) = snapshot
            .active
            .as_ref()
            .map(|record| record.identity.clone())
            .or_else(|| {
                snapshot
                    .latest
                    .as_ref()
                    .map(|record| record.identity.clone())
            })
            .or_else(|| {
                snapshot
                    .stale_active
                    .as_ref()
                    .map(|record| record.identity.clone())
            })
        else {
            continue;
        };
        if fs_same_path(&identity.repo_root, repo_root) {
            rows.push(build_inventory_row(
                repo_root,
                identity.resolved_selector.clone(),
                identity.selected_catalog_root.clone(),
                false,
                true,
                snapshot,
            ));
        }
    }

    rows.sort_by(|left, right| {
        left.selected_catalog_root
            .cmp(&right.selected_catalog_root)
            .then_with(|| left.selector.cmp(&right.selector))
    });
    Ok(rows)
}

fn declared_task_targets(
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
) -> BTreeMap<String, DeclaredTaskStatusTarget> {
    let mut targets = BTreeMap::new();
    for catalog in catalogs {
        for task_name in catalog.manifest.tasks.keys() {
            let selector = selector_label(catalog, task_name);
            let identity = TaskStatusTargetIdentity::new(
                repo_root.to_path_buf(),
                catalog.catalog_root.clone(),
                selector.clone(),
                task_name.clone(),
                None,
            );
            targets.insert(
                identity.status_key().as_str().to_owned(),
                DeclaredTaskStatusTarget {
                    identity,
                    selector,
                    selected_catalog_root: catalog.catalog_root.clone(),
                },
            );
        }
    }
    targets
}

fn build_inventory_row(
    repo_root: &Path,
    selector: String,
    selected_catalog_root: PathBuf,
    currently_declared: bool,
    no_longer_declared: bool,
    snapshot: TaskStatusReadSnapshot,
) -> TaskStatusInventoryRow {
    let state = effective_state(&snapshot);
    let last_updated = snapshot
        .active
        .as_ref()
        .map(|record| record.updated_at.clone())
        .or_else(|| {
            snapshot
                .latest
                .as_ref()
                .map(|record| record.finished_at.clone())
        })
        .or_else(|| {
            snapshot
                .stale_active
                .as_ref()
                .map(|record| record.updated_at.clone())
        });
    let route = snapshot
        .active
        .as_ref()
        .map(|record| render_route_summary(&record.runtime_route))
        .or_else(|| {
            snapshot
                .latest
                .as_ref()
                .map(|record| render_route_summary(&record.runtime_route))
        })
        .or_else(|| {
            snapshot
                .stale_active
                .as_ref()
                .map(|record| render_route_summary(&record.runtime_route))
        });

    TaskStatusInventoryRow {
        selector,
        selected_catalog_root: relative_or_absolute(repo_root, &selected_catalog_root),
        state: state_label(state).to_owned(),
        currently_declared,
        no_longer_declared: no_longer_declared.then_some(true),
        last_updated,
        route,
        active: snapshot.active,
        latest: snapshot.latest,
        stale_active: snapshot.stale_active,
        warnings: snapshot.warnings,
    }
}

fn render_task_status_json(
    repo_root: &Path,
    plan: &ExecutionSelectionPlan,
    resolved_selector: &str,
    state: TaskStatusState,
    snapshot: &TaskStatusReadSnapshot,
) -> Result<String, RunnerError> {
    let payload = json!({
        "schema": "effigy.tasks-status.v1",
        "schema_version": 1,
        "resolved_selector": resolved_selector,
        "selected_catalog_root": plan.catalog.catalog_root.display().to_string(),
        "state": state_label(state),
        "currently_declared": true,
        "active": snapshot.active,
        "latest": snapshot.latest,
        "stale_active": snapshot.stale_active,
        "warnings": snapshot.warnings,
        "routing": {
            "repo_root": repo_root.display().to_string(),
            "catalog_alias": plan.catalog.alias.clone(),
            "catalog_root": plan.catalog.catalog_root.display().to_string(),
            "catalog_manifest_path": plan.catalog.manifest_path.display().to_string(),
            "selection_mode": selection_mode_label(plan.mode),
            "evidence": plan.evidence.clone(),
        }
    });
    serde_json::to_string_pretty(&payload).map_err(|error| {
        RunnerError::task_invocation(format!("failed to render task status json: {error}"))
    })
}

fn render_task_status_all_json(
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    rows: &[TaskStatusInventoryRow],
) -> Result<String, RunnerError> {
    let mut counts = BTreeMap::<String, usize>::new();
    for row in rows {
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

    let payload = json!({
        "schema": "effigy.tasks-status-all.v1",
        "schema_version": 1,
        "scope_root": repo_root.display().to_string(),
        "catalog_scopes": catalog_scopes,
        "counts_by_state": counts,
        "warnings": [],
        "rows": rows,
    });
    serde_json::to_string_pretty(&payload).map_err(|error| {
        RunnerError::task_invocation(format!(
            "failed to render task status inventory json: {error}"
        ))
    })
}

fn render_task_status_text(
    repo_root: &Path,
    plan: &ExecutionSelectionPlan,
    resolved_selector: &str,
    state: TaskStatusState,
    snapshot: &TaskStatusReadSnapshot,
) -> String {
    let mut renderer = text_renderer();
    let _ = renderer.section("Task Status");
    let _ = renderer.key_values(&[
        effigy_core::widgets::KeyValue::new("task", resolved_selector.to_owned()),
        effigy_core::widgets::KeyValue::new("state", state_label(state).to_owned()),
        effigy_core::widgets::KeyValue::new("catalog", plan.catalog.alias.clone()),
        effigy_core::widgets::KeyValue::new("repo-root", repo_root.display().to_string()),
        effigy_core::widgets::KeyValue::new(
            "catalog-root",
            plan.catalog.catalog_root.display().to_string(),
        ),
    ]);

    if let Some(active) = &snapshot.active {
        let _ = renderer.text("");
        let _ = renderer.key_values(&[
            effigy_core::widgets::KeyValue::new("stage", stage_label(active.stage).to_owned()),
            effigy_core::widgets::KeyValue::new("pid", active.owner_pid.to_string()),
            effigy_core::widgets::KeyValue::new("updated-at", active.updated_at.clone()),
            effigy_core::widgets::KeyValue::new(
                "route",
                render_route_summary(&active.runtime_route),
            ),
        ]);
    } else if let Some(latest) = &snapshot.latest {
        let _ = renderer.text("");
        let _ = renderer.key_values(&[
            effigy_core::widgets::KeyValue::new("finished-at", latest.finished_at.clone()),
            effigy_core::widgets::KeyValue::new(
                "duration-ms",
                latest
                    .duration_ms
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_owned()),
            ),
            effigy_core::widgets::KeyValue::new("outcome", latest.outcome.summary.clone()),
            effigy_core::widgets::KeyValue::new(
                "route",
                render_route_summary(&latest.runtime_route),
            ),
        ]);
    } else {
        let _ = renderer.text("");
        let _ = renderer.text("No recorded task status yet.");
    }

    if !snapshot.warnings.is_empty() {
        let _ = renderer.text("");
        let lines = snapshot
            .warnings
            .iter()
            .map(|warning| format!("{}: {}", warning.code, warning.message))
            .collect::<Vec<_>>();
        let _ = renderer.bullet_list("warnings", &lines);
    }

    if !plan.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("routing-evidence", &plan.evidence);
    }
    String::from_utf8_lossy(&renderer.into_inner()).to_string()
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
            if let Some(route) = &row.route {
                line.push_str(&format!(" {route}"));
            }
            if let Some(last_updated) = &row.last_updated {
                line.push_str(&format!(" updated={last_updated}"));
            }
            let _ = renderer.text(&line);
            for warning in &row.warnings {
                let _ = renderer.text(&format!("  warning: {}: {}", warning.code, warning.message));
            }
        }
    }

    String::from_utf8_lossy(&renderer.into_inner()).to_string()
}

fn effective_state(snapshot: &TaskStatusReadSnapshot) -> TaskStatusState {
    snapshot
        .active
        .as_ref()
        .map(|record| record.state)
        .or_else(|| snapshot.latest.as_ref().map(|record| record.state))
        .unwrap_or(TaskStatusState::Unknown)
}

fn selector_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

fn relative_or_absolute(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
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

fn fs_same_path(left: &Path, right: &Path) -> bool {
    std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf())
        == std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf())
}

fn state_label(state: TaskStatusState) -> &'static str {
    match state {
        TaskStatusState::Running => "running",
        TaskStatusState::Succeeded => "succeeded",
        TaskStatusState::Failed => "failed",
        TaskStatusState::Cancelled => "cancelled",
        TaskStatusState::Blocked => "blocked",
        TaskStatusState::Unknown => "unknown",
    }
}

fn stage_label(stage: effigy_execution::TaskStatusStage) -> &'static str {
    match stage {
        effigy_execution::TaskStatusStage::Routing => "routing",
        effigy_execution::TaskStatusStage::WaitingForLock => "waiting-for-lock",
        effigy_execution::TaskStatusStage::RuntimePrep => "runtime-prep",
        effigy_execution::TaskStatusStage::Executing => "executing",
        effigy_execution::TaskStatusStage::ManagedSession => "managed-session",
        effigy_execution::TaskStatusStage::Handoff => "handoff",
        effigy_execution::TaskStatusStage::Finishing => "finishing",
    }
}

fn selection_mode_label(mode: CatalogSelectionMode) -> &'static str {
    match mode {
        CatalogSelectionMode::ExplicitPrefix => "explicit-prefix",
        CatalogSelectionMode::CwdNearest => "cwd-nearest",
        CatalogSelectionMode::RootShallowest => "root-shallowest",
    }
}

fn render_route_summary(summary: &effigy_execution::TaskStatusRuntimeRouteSummary) -> String {
    match (&summary.container, &summary.service) {
        (Some(container), Some(service)) => format!("{} ({container}/{service})", summary.route),
        (Some(container), None) => format!("{} ({container})", summary.route),
        (None, _) => summary.route.clone(),
    }
}
