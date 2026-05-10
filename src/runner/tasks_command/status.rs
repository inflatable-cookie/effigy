use effigy_cli::TasksArgs;
use effigy_execution::{
    ExecutionSelectionCatalogSummary, ExecutionSelectionInput, ExecutionSelectionPlan,
    TaskStatusState, TaskStatusTargetIdentity,
};
use effigy_runtime::task_status::{reconcile_task_status_records, TaskStatusReadSnapshot};
use effigy_tasks::{parse_task_selector, render_task_selector, CatalogSelectionMode};
use effigy_ui::{text_renderer, Renderer};

use crate::runner::command_context::resolve_active_command_context;
use crate::runner::error::RunnerError;
use effigy_routing::select_catalog_and_task;
use serde_json::json;

pub(super) fn run_task_status(args: &TasksArgs, raw_selector: &str) -> Result<String, RunnerError> {
    let context = resolve_active_command_context(args.repo_override.clone())?;
    let catalogs =
        effigy_routing::discover_catalogs_allow_missing(&context.resolved.resolved_root)?;
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
    let state = snapshot
        .active
        .as_ref()
        .map(|record| record.state)
        .or_else(|| snapshot.latest.as_ref().map(|record| record.state))
        .unwrap_or(TaskStatusState::Unknown);

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

fn render_task_status_json(
    repo_root: &std::path::Path,
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

fn render_task_status_text(
    repo_root: &std::path::Path,
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
