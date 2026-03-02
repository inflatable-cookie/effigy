use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use super::super::super::execute::{catalog_task_label, task_run_preview};
use super::super::super::tasks_view::{
    managed_profile_display_rows, relative_display_path, style_text,
};
use super::super::super::{LoadedCatalog, RunnerError, BUILTIN_TASKS};
use super::super::catalog_rows::{assemble_catalog_rows, CatalogRow};
use super::rows::{render_builtin_task_rows, render_task_with_profiles};

pub(super) fn render_default_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    render_catalogs_section(
        renderer,
        color_enabled,
        theme,
        catalogs,
        ordered_catalogs,
        resolved_root,
    )?;
    render_tasks_section(
        renderer,
        color_enabled,
        theme,
        ordered_catalogs,
        resolved_root,
    )?;
    renderer.section("Built-in Tasks")?;
    render_builtin_task_rows(renderer, color_enabled, theme, &BUILTIN_TASKS)?;
    Ok(())
}

fn render_catalogs_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    renderer.section("Catalogs")?;
    renderer.key_values(&[KeyValue::new("count", catalogs.len().to_string())])?;
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        for catalog in ordered_catalogs {
            let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
            renderer.text(&format!(
                "- {} : {}",
                style_text(color_enabled, theme.task_name, &catalog.alias),
                style_text(color_enabled, theme.muted, &manifest),
            ))?;
        }
    }
    renderer.text("")?;
    Ok(())
}

fn render_tasks_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    renderer.section("Tasks")?;
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        let rows = assemble_catalog_rows(ordered_catalogs);
        for row in rows.rows() {
            if let CatalogRow::Task {
                catalog,
                task_name,
                task,
            } = row
            {
                let manifest = relative_display_path(resolved_root, &catalog.manifest_path);
                render_task_with_profiles(
                    renderer,
                    color_enabled,
                    theme,
                    &manifest,
                    &catalog_task_label(catalog, task_name),
                    &task_run_preview(task),
                    managed_profile_display_rows(catalog, task_name, task),
                )?;
            }
        }
        if !rows.has_tasks() {
            renderer.notice(NoticeLevel::Info, "none")?;
        }
    }
    renderer.text("")?;
    Ok(())
}
