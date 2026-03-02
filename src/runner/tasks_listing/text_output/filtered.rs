use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::tasks_view::{relative_display_path, render_resolution_probe_block};
use super::super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::super::filtering::evaluate_task_filter;
use super::rows::{render_builtin_task_rows, render_task_with_profiles};

pub(super) fn render_filtered_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    filter: &str,
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    let evaluation = evaluate_task_filter(catalogs, filter)?;
    renderer.section(&format!("Task Matches: {filter}"))?;

    if evaluation.catalog_matches.is_empty() && evaluation.builtin_matches.is_empty() {
        renderer.notice(NoticeLevel::Warning, "no matches")?;
        return Ok(());
    }

    render_filtered_catalog_task_matches(
        renderer,
        color_enabled,
        theme,
        resolved_root,
        evaluation.catalog_matches.as_slice(),
        &evaluation.task_name,
    )?;

    if !evaluation.builtin_matches.is_empty() || resolve_probe.is_some() {
        renderer.text("")?;
    }
    if !evaluation.builtin_matches.is_empty() {
        renderer.section("Built-in Task Matches")?;
        render_builtin_task_rows(
            renderer,
            color_enabled,
            theme,
            evaluation.builtin_matches.as_slice(),
        )?;
        render_builtin_test_fallback_notice(renderer, evaluation.notes.as_slice())?;
        if resolve_probe.is_some() {
            renderer.text("")?;
        }
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(renderer, probe, color_enabled, false)?;
    }
    Ok(())
}

fn render_filtered_catalog_task_matches(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    resolved_root: &Path,
    matches: &[(&LoadedCatalog, &ManifestTask)],
    task_name: &str,
) -> Result<(), RunnerError> {
    for (catalog, task) in matches {
        let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
        render_task_with_profiles(
            renderer,
            color_enabled,
            theme,
            &manifest,
            catalog,
            task_name,
            task,
        )?;
    }
    Ok(())
}

fn render_builtin_test_fallback_notice(
    renderer: &mut PlainRenderer<Vec<u8>>,
    notes: &[String],
) -> Result<(), RunnerError> {
    for note in notes {
        renderer.notice(NoticeLevel::Info, note)?;
    }
    Ok(())
}
