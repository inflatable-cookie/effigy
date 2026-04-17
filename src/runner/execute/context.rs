use std::path::{Path, PathBuf};

use effigy_core::resolver::ResolvedTarget;
use effigy_core::widgets::KeyValue;
use effigy_manifest::TaskSelection;
use effigy_tasks::TaskSelector;
use effigy_ui::{text_renderer, Renderer};

use super::preflight::ExecutionPreflight;

pub(super) struct ExecutionTaskContext<'a> {
    pub(super) resolved: &'a ResolvedTarget,
    pub(super) selector: &'a TaskSelector,
    pub(super) selection: &'a TaskSelection<'a>,
    pub(super) resolved_root: &'a Path,
    pub(super) repo_for_task: PathBuf,
    pub(super) command: String,
}

impl<'a> ExecutionTaskContext<'a> {
    pub(super) fn new(
        preflight: &'a ExecutionPreflight,
        selection: &'a TaskSelection<'a>,
        command: String,
    ) -> Self {
        Self {
            resolved: &preflight.resolved,
            selector: &preflight.selector,
            selection,
            resolved_root: &preflight.resolved.resolved_root,
            repo_for_task: selection.catalog.catalog_root.clone(),
            command,
        }
    }

    pub(super) fn command(&self) -> &str {
        &self.command
    }

    pub(super) fn repo_for_task(&self) -> &Path {
        &self.repo_for_task
    }

    pub(super) fn render_resolution_trace(&self) -> String {
        render_task_resolution_trace(
            self.resolved,
            self.selector,
            self.selection,
            self.repo_for_task(),
            self.command(),
        )
    }
}

fn render_task_resolution_trace(
    resolved: &ResolvedTarget,
    selector: &TaskSelector,
    selection: &TaskSelection<'_>,
    execution_cwd: &Path,
    command: &str,
) -> String {
    let mut renderer = text_renderer();
    let _ = renderer.section("Task Resolution");
    let mut values = vec![
        KeyValue::new("task", selector.task_name.clone()),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("root-mode", format!("{:?}", resolved.resolution_mode)),
        KeyValue::new("catalog-alias", selection.catalog.alias.clone()),
        KeyValue::new(
            "catalog-path",
            selection.catalog.manifest_path.display().to_string(),
        ),
        KeyValue::new("catalog-mode", format!("{:?}", selection.mode)),
        KeyValue::new("execution-cwd", execution_cwd.display().to_string()),
        KeyValue::new("command", command.to_owned()),
    ];
    if let Some(prefix) = &selector.prefix {
        values.insert(1, KeyValue::new("prefix", prefix.clone()));
    }
    let _ = renderer.key_values(&values);
    if !resolved.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-evidence", &resolved.evidence);
    }
    if !resolved.warnings.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-warnings", &resolved.warnings);
    }
    if !selection.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("catalog-evidence", &selection.evidence);
    }
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}
