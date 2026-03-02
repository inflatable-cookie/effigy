use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use super::execute::catalog_task_label;
use super::{LoadedCatalog, ManifestTask, RunnerError};

#[derive(Debug)]
pub(super) struct ManagedProfileDisplayRow {
    pub(super) task: String,
    pub(super) run: String,
    pub(super) profile: String,
    pub(super) invocation: String,
    pub(super) parent_task: String,
}

pub(super) fn relative_display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(super) fn managed_profile_display_rows(
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
        .filter(|profile| profile.as_str() != "default")
        .map(|profile| ManagedProfileDisplayRow {
            task: format!("{parent_task} {profile}"),
            run: format!("<managed:{mode} profile:{profile}>"),
            profile: profile.clone(),
            invocation: format!("{parent_task} {profile}"),
            parent_task: parent_task.clone(),
        })
        .collect()
}

pub(super) fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
}

fn probe_text_field<'a>(probe: &'a serde_json::Value, field: &str, fallback: &'a str) -> &'a str {
    probe[field].as_str().unwrap_or(fallback)
}

fn probe_lines_field(probe: &serde_json::Value, field: &str) -> Vec<String> {
    probe[field]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect::<Vec<String>>()
}

fn render_probe_lock_scopes(probe: &serde_json::Value) -> String {
    let scopes = probe_lines_field(probe, "lock_scopes");
    if scopes.is_empty() {
        return "<none>".to_owned();
    }
    scopes.join(", ")
}

fn render_probe_evidence_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    probe: &serde_json::Value,
) -> Result<(), RunnerError> {
    let lines = probe_lines_field(probe, "evidence");
    if lines.is_empty() {
        return Ok(());
    }
    let theme = Theme::default();
    renderer.text(&format!(
        "{}:",
        style_text(color_enabled, theme.label, "evidence")
    ))?;
    for line in lines {
        renderer.text(&format!("- {line}"))?;
    }
    Ok(())
}

pub(super) fn render_resolution_probe_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    probe: &serde_json::Value,
    color_enabled: bool,
    show_evidence: bool,
) -> Result<(), RunnerError> {
    renderer.section(&format!(
        "Resolution: {}",
        probe_text_field(probe, "selector", "<selector>")
    ))?;
    renderer.key_values(&[
        KeyValue::new("status", probe_text_field(probe, "status", "<unknown>")),
        KeyValue::new("catalog", probe_text_field(probe, "catalog", "<none>")),
        KeyValue::new("task", probe_text_field(probe, "task", "<none>")),
        KeyValue::new("lock_scopes", render_probe_lock_scopes(probe)),
    ])?;
    if let Some(error) = probe["error"].as_str().filter(|value| !value.is_empty()) {
        renderer.notice(NoticeLevel::Warning, error)?;
        return Ok(());
    }
    if !show_evidence {
        return Ok(());
    }
    render_probe_evidence_block(renderer, color_enabled, probe)
}
