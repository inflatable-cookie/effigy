use effigy_ui::theme::Theme;
use effigy_ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use crate::runner::error::RunnerError;

fn render_probe_evidence_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    evidence_lines: &[String],
) -> Result<(), RunnerError> {
    if evidence_lines.is_empty() {
        return Ok(());
    }
    let theme = Theme::default();
    renderer.text(&format!(
        "{}:",
        super::style_text(color_enabled, theme.label, "evidence")
    ))?;
    for line in evidence_lines {
        renderer.text(&format!("- {line}"))?;
    }
    Ok(())
}

pub(in crate::runner) fn render_resolution_probe_block(
    renderer: &mut PlainRenderer<Vec<u8>>,
    probe: &serde_json::Value,
    color_enabled: bool,
    show_evidence: bool,
) -> Result<(), RunnerError> {
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
                .filter(|v| !v.is_empty()),
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
        .collect::<Vec<String>>()
}
