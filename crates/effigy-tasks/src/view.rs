use std::path::Path;

use effigy_core::widgets::{KeyValue, NoticeLevel};
use effigy_manifest::{
    resolve_task_execution_binding, LoadedCatalog, ManifestManagedRun, ManifestTask,
    ResolvedTaskExecutionBinding,
};
use effigy_ui::theme::Theme;
use effigy_ui::{PlainRenderer, Renderer};

use crate::EffigyTasksError;

const DEFAULT_MANAGED_PROFILE: &str = "default";

#[derive(Debug, Clone, serde::Serialize)]
pub struct ManagedProfileDisplayRow {
    pub task: String,
    pub run: String,
    pub profile: String,
    pub invocation: String,
    pub parent_task: String,
}

pub fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

pub fn relative_display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

pub fn task_run_preview(catalog: &LoadedCatalog, task_name: &str, task: &ManifestTask) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Ok(Some(binding)) = resolve_task_execution_binding(&catalog.manifest, task_name, task) {
        return match binding {
            ResolvedTaskExecutionBinding::Host => "<host>".to_owned(),
            ResolvedTaskExecutionBinding::Workspace(binding) => {
                format!("<workspace:{}:{}>", binding.system, binding.workspace)
            }
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

pub fn managed_profile_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<ManagedProfileDisplayRow> {
    let Some(mode) = task.mode.as_deref() else {
        return Vec::new();
    };
    if task.profiles.is_empty() {
        return Vec::new();
    }
    let parent_task = catalog_task_label(catalog, task_name);
    task.profiles
        .keys()
        .filter(|profile| profile.as_str() != DEFAULT_MANAGED_PROFILE)
        .map(|profile| ManagedProfileDisplayRow {
            task: format!("{parent_task} {profile}"),
            run: format!("<managed:{mode} profile:{profile}>"),
            profile: profile.clone(),
            invocation: format!("{parent_task} {profile}"),
            parent_task: parent_task.clone(),
        })
        .collect()
}

fn render_probe_evidence_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    evidence_lines: &[String],
) -> Result<(), EffigyTasksError> {
    if evidence_lines.is_empty() {
        return Ok(());
    }
    let theme = Theme::default();
    renderer.text(&format!(
        "{}:",
        style_text(color_enabled, theme.label, "evidence")
    ))?;
    for line in evidence_lines {
        renderer.text(&format!("- {line}"))?;
    }
    Ok(())
}

pub fn render_resolution_probe_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    probe: &serde_json::Value,
    color_enabled: bool,
    show_evidence: bool,
) -> Result<(), EffigyTasksError> {
    let view = ResolutionProbeView::from_value(probe);
    let lock_scopes = view.lock_scopes_display();
    renderer.section(&format!("Resolution: {}", view.selector))?;
    renderer.key_values(&[
        KeyValue::new("status", view.status.as_str()),
        KeyValue::new("catalog", view.catalog.as_str()),
        KeyValue::new("task", view.task.as_str()),
        KeyValue::new("lock_scopes", lock_scopes),
    ])?;
    if let Some(error) = view.error {
        renderer.notice(NoticeLevel::Warning, &error)?;
        return Ok(());
    }
    if !show_evidence {
        return Ok(());
    }
    render_probe_evidence_block(renderer, color_enabled, view.evidence.as_slice())
}

struct ResolutionProbeView {
    selector: String,
    status: String,
    catalog: String,
    task: String,
    lock_scopes: Vec<String>,
    evidence: Vec<String>,
    error: Option<String>,
}

impl ResolutionProbeView {
    fn from_value(probe: &serde_json::Value) -> Self {
        Self {
            selector: probe_str(probe, "selector", "<selector>"),
            status: probe_str(probe, "status", "<unknown>"),
            catalog: probe_str(probe, "catalog", "<none>"),
            task: probe_str(probe, "task", "<none>"),
            lock_scopes: probe_lines(probe, "lock_scopes"),
            evidence: probe_lines(probe, "evidence"),
            error: probe["error"]
                .as_str()
                .map(str::to_owned)
                .filter(|value| !value.is_empty()),
        }
    }

    fn lock_scopes_display(&self) -> String {
        if self.lock_scopes.is_empty() {
            return "<none>".to_owned();
        }
        self.lock_scopes.join(", ")
    }
}

fn probe_str(probe: &serde_json::Value, field: &str, fallback: &str) -> String {
    probe[field].as_str().unwrap_or(fallback).to_owned()
}

fn probe_lines(probe: &serde_json::Value, field: &str) -> Vec<String> {
    probe[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect()
}
