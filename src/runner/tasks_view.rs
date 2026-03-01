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

fn render_probe_lock_scopes(probe: &serde_json::Value) -> String {
    let Some(scopes) = probe["lock_scopes"].as_array() else {
        return "<none>".to_owned();
    };
    let rendered = scopes
        .iter()
        .filter_map(|value| value.as_str())
        .map(str::to_owned)
        .collect::<Vec<String>>();
    if rendered.is_empty() {
        return "<none>".to_owned();
    }
    rendered.join(", ")
}

pub(super) fn render_resolution_probe_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    probe: &serde_json::Value,
    color_enabled: bool,
    show_evidence: bool,
) -> Result<(), RunnerError> {
    renderer.section(&format!(
        "Resolution: {}",
        probe["selector"].as_str().unwrap_or("<selector>")
    ))?;
    renderer.key_values(&[
        KeyValue::new("status", probe["status"].as_str().unwrap_or("<unknown>")),
        KeyValue::new("catalog", probe["catalog"].as_str().unwrap_or("<none>")),
        KeyValue::new("task", probe["task"].as_str().unwrap_or("<none>")),
        KeyValue::new("lock_scopes", render_probe_lock_scopes(probe)),
    ])?;
    if let Some(error) = probe["error"].as_str() {
        renderer.notice(NoticeLevel::Warning, error)?;
        return Ok(());
    }
    if !show_evidence {
        return Ok(());
    }
    let Some(evidence) = probe["evidence"].as_array() else {
        return Ok(());
    };
    let lines = evidence
        .iter()
        .filter_map(|item| item.as_str().map(str::to_owned))
        .collect::<Vec<String>>();
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
