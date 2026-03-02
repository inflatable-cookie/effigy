use std::io::IsTerminal;

use serde_json::json;

use crate::ui::theme::resolve_color_enabled;
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;

use super::super::super::{LoadedCatalog, RunnerError};
use super::analysis::map_render_error;
use super::{DeferralOutcome, SelectionOutcome};

pub(super) fn render_explain_json(
    request: &TaskInvocation,
    resolved: &crate::resolver::ResolvedTarget,
    task_name: &str,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
) -> Result<String, RunnerError> {
    let payload = json!({
        "schema": "effigy.doctor.explain.v1",
        "schema_version": 1,
        "request": {
            "task": request.name,
            "args": request.args,
        },
        "root_resolution": {
            "resolved_root": resolved.resolved_root.display().to_string(),
            "evidence": resolved.evidence,
            "warnings": resolved.warnings,
        },
        "selection": {
            "status": selection.status,
            "catalog": selection.catalog,
            "task": task_name,
            "mode": selection.mode,
            "evidence": selection.evidence,
            "error": selection.error,
        },
        "candidates": candidates,
        "ambiguity_candidates": selection.ambiguity_candidates,
        "deferral": {
            "considered": deferral.considered,
            "selected": deferral.selected,
            "source": deferral.source,
            "working_dir": deferral.working_dir,
        },
        "reasoning": {
            "selection": selection.reasoning,
            "deferral": deferral.reasoning,
        },
    });
    serde_json::to_string_pretty(&payload)
        .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
}

pub(super) fn render_explain_text(
    request: &TaskInvocation,
    resolved: &crate::resolver::ResolvedTarget,
    selection: &SelectionOutcome,
    deferral: &DeferralOutcome,
    candidates: &[String],
    catalogs: &[LoadedCatalog],
    verbose: bool,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer
        .section("Doctor Explain")
        .map_err(map_render_error)?;
    renderer
        .key_values(&[
            KeyValue::new("request", request.name.clone()),
            KeyValue::new("args", request.args.join(" ")),
            KeyValue::new(
                "resolved-root",
                resolved.resolved_root.display().to_string(),
            ),
            KeyValue::new("selection-status", selection.status.clone()),
            KeyValue::new(
                "selected-catalog",
                selection
                    .catalog
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
            KeyValue::new(
                "selected-mode",
                selection
                    .mode
                    .clone()
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
            KeyValue::new("selection-reasoning", selection.reasoning.clone()),
            KeyValue::new("deferral-considered", deferral.considered.to_string()),
            KeyValue::new("deferral-selected", deferral.selected.to_string()),
            KeyValue::new("deferral-reasoning", deferral.reasoning.clone()),
        ])
        .map_err(map_render_error)?;
    if let Some(source) = deferral.source.as_ref() {
        renderer
            .key_values(&[KeyValue::new("deferral-source", source.clone())])
            .map_err(map_render_error)?;
    }
    if let Some(working_dir) = deferral.working_dir.as_ref() {
        renderer
            .key_values(&[KeyValue::new("deferral-working-dir", working_dir.clone())])
            .map_err(map_render_error)?;
    }
    if let Some(error) = selection.error.as_ref() {
        renderer
            .notice(NoticeLevel::Warning, error)
            .map_err(map_render_error)?;
    }
    renderer.text("").map_err(map_render_error)?;
    renderer
        .bullet_list("candidate-catalogs", candidates)
        .map_err(map_render_error)?;
    if !selection.evidence.is_empty() {
        renderer
            .bullet_list("selection-evidence", &selection.evidence)
            .map_err(map_render_error)?;
    }
    if !selection.ambiguity_candidates.is_empty() {
        renderer
            .bullet_list("ambiguity-candidates", &selection.ambiguity_candidates)
            .map_err(map_render_error)?;
    }
    if verbose {
        let mut all_catalogs = catalogs
            .iter()
            .map(|catalog| {
                format!(
                    "{} ({}) depth={} has_defer={}",
                    catalog.alias,
                    catalog.manifest_path.display(),
                    catalog.depth,
                    catalog.defer_run.is_some()
                )
            })
            .collect::<Vec<String>>();
        all_catalogs.sort();
        renderer
            .bullet_list("discovered-catalogs", &all_catalogs)
            .map_err(map_render_error)?;
        if !resolved.evidence.is_empty() {
            renderer
                .bullet_list("root-resolution-evidence", &resolved.evidence)
                .map_err(map_render_error)?;
        }
        if !resolved.warnings.is_empty() {
            renderer
                .bullet_list("root-resolution-warnings", &resolved.warnings)
                .map_err(map_render_error)?;
        }
    }
    let out = renderer.into_inner();
    Ok(String::from_utf8_lossy(&out).to_string())
}
