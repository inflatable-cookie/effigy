use serde_json::json;

use effigy_manifest::TaskSelection;
use effigy_tasks::{TaskSelector, TaskSurface};

/// The resolved runtime target a selector plan exposes.
///
/// This is the same authoritative target the execution path routes to, so
/// `--plan` and preflight surfaces cannot disagree with execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum PlannedRuntimeTarget {
    Host,
    Container {
        container: String,
        service: String,
        root: std::path::PathBuf,
    },
    InlineContainer,
}

impl PlannedRuntimeTarget {
    pub(in crate::runner) fn to_json(&self) -> serde_json::Value {
        match self {
            Self::Host => json!({ "target": "host" }),
            Self::InlineContainer => json!({ "target": "inline-container" }),
            Self::Container {
                container,
                service,
                root,
            } => json!({
                "target": "container",
                "container": container,
                "service": service,
                "root": root.display().to_string(),
            }),
        }
    }
}

pub(super) fn task_run_payload(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    exit_code: Option<i32>,
    stdout: &str,
    stderr: &str,
    selection: &TaskSelection<'_>,
) -> serde_json::Value {
    let mut payload = json!({
        "schema": "effigy.task.run.v1",
        "schema_version": 1,
        "ok": exit_code == Some(0),
        "task": selector.task_name,
        "selector": render_selector(selector),
        "command": command,
        "cwd": cwd.display().to_string(),
        "exit_code": exit_code,
        "stdout": stdout,
        "stderr": stderr,
    });

    // Additive draft metadata. Published task-run payloads stay byte- and
    // schema-compatible: these fields appear only for an explicit draft run.
    if selection.surface == TaskSurface::Draft {
        if let Some(object) = payload.as_object_mut() {
            object.insert("surface".to_owned(), json!("draft"));
            object.insert(
                "surface_identity".to_owned(),
                json!({
                    "surface": "draft",
                    "catalog_alias": selection.catalog.alias,
                    "catalog_root": selection.catalog.catalog_root.display().to_string(),
                    "definition_source": selection
                        .catalog
                        .draft_source(&selector.task_name)
                        .display()
                        .to_string(),
                }),
            );
        }
    }

    payload
}

fn render_selector(selector: &TaskSelector) -> String {
    selector
        .prefix
        .as_ref()
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or_else(|| selector.task_name.clone())
}

pub(super) fn task_plan_payload(
    selector: &TaskSelector,
    cwd: &std::path::Path,
    command: &str,
    selection: &TaskSelection<'_>,
    runtime: Option<&PlannedRuntimeTarget>,
) -> serde_json::Value {
    let mut payload = json!({
        "schema": "effigy.task.plan.v1",
        "schema_version": 1,
        "ok": true,
        "executed": false,
        "task": selector.task_name,
        "selector": render_selector(selector),
        "command": command,
        "cwd": cwd.display().to_string(),
        "catalog": {
            "alias": selection.catalog.alias,
            "root": selection.catalog.catalog_root.display().to_string(),
            "manifest": selection.catalog.manifest_path.display().to_string(),
        },
    });

    if let Some(runtime) = runtime {
        if let Some(object) = payload.as_object_mut() {
            object.insert("runtime".to_owned(), runtime.to_json());
        }
    }

    if selection.surface == TaskSurface::Draft {
        if let Some(object) = payload.as_object_mut() {
            object.insert("surface".to_owned(), json!("draft"));
            object.insert(
                "surface_identity".to_owned(),
                json!({
                    "surface": "draft",
                    "catalog_alias": selection.catalog.alias,
                    "catalog_root": selection.catalog.catalog_root.display().to_string(),
                    "definition_source": selection
                        .catalog
                        .draft_source(&selector.task_name)
                        .display()
                        .to_string(),
                }),
            );
        }
    }

    payload
}
