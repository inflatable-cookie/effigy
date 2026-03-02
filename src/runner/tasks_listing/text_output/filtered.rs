use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::execute::{catalog_task_label, task_run_preview};
use super::super::super::tasks_view::{
    managed_profile_display_rows, relative_display_path, render_resolution_probe_block,
};
use super::super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::super::matches::{builtin_matches, matched_catalog_tasks};
use super::super::BUILTIN_TEST_FALLBACK_NOTE;
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
    let selector = super::super::super::util::parse_task_selector(filter)?;
    renderer.section(&format!("Task Matches: {filter}"))?;

    let matches = matched_catalog_tasks(catalogs, &selector);
    let builtin_matches = builtin_matches(&selector);

    if matches.is_empty() && builtin_matches.is_empty() {
        renderer.notice(NoticeLevel::Warning, "no matches")?;
        return Ok(());
    }

    render_filtered_catalog_task_matches(
        renderer,
        color_enabled,
        theme,
        resolved_root,
        matches.as_slice(),
        &selector.task_name,
    )?;

    if !builtin_matches.is_empty() || resolve_probe.is_some() {
        renderer.text("")?;
    }
    if !builtin_matches.is_empty() {
        renderer.section("Built-in Task Matches")?;
        render_builtin_task_rows(renderer, color_enabled, theme, builtin_matches.as_slice())?;
        render_builtin_test_fallback_notice(renderer, &selector.task_name)?;
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
        let task_label = catalog_task_label(catalog, task_name);
        let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
        let signature = task_run_preview(task);
        render_task_with_profiles(
            renderer,
            color_enabled,
            theme,
            &manifest,
            &task_label,
            &signature,
            managed_profile_display_rows(catalog, task_name, task),
        )?;
    }
    Ok(())
}

fn render_builtin_test_fallback_notice(
    renderer: &mut PlainRenderer<Vec<u8>>,
    task_name: &str,
) -> Result<(), RunnerError> {
    if task_name == "test" {
        renderer.notice(NoticeLevel::Info, BUILTIN_TEST_FALLBACK_NOTE)?;
    }
    Ok(())
}
