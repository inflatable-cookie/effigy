use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use super::super::super::{LoadedCatalog, RunnerError};
use super::super::catalog_manifest::{ordered_manifest_display_contexts, CatalogManifestContext};
use super::super::row_projection::{builtin_task_rows, BuiltinTaskRow};
use super::rows::{
    render_builtin_task_rows, render_catalog_alias_rows, render_ordered_catalog_task_rows,
};

pub(super) fn render_default_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    let display_contexts: Vec<CatalogManifestContext<'_>> =
        ordered_manifest_display_contexts(ordered_catalogs, resolved_root).collect();
    render_catalogs_section(
        renderer,
        color_enabled,
        theme,
        catalogs,
        display_contexts.as_slice(),
    )?;
    render_tasks_section(renderer, color_enabled, theme, display_contexts.as_slice())?;
    render_builtin_rows_section(
        renderer,
        color_enabled,
        theme,
        "Built-in Tasks",
        builtin_task_rows(),
        &[],
    )?;
    Ok(())
}

pub(super) fn render_builtin_rows_section<'a>(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    title: &str,
    rows: impl IntoIterator<Item = BuiltinTaskRow<'a>>,
    notes: &[String],
) -> Result<(), RunnerError> {
    renderer.section(title)?;
    render_builtin_task_rows(renderer, color_enabled, theme, rows)?;
    render_info_notices(renderer, notes)
}

fn render_catalogs_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    display_contexts: &[CatalogManifestContext<'_>],
) -> Result<(), RunnerError> {
    render_catalog_scoped_section(
        renderer,
        "Catalogs",
        display_contexts,
        |renderer| {
            renderer.key_values(&[KeyValue::new("count", catalogs.len().to_string())])?;
            Ok(())
        },
        |renderer| render_catalog_alias_rows(renderer, color_enabled, theme, display_contexts),
    )
}

fn render_tasks_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    display_contexts: &[CatalogManifestContext<'_>],
) -> Result<(), RunnerError> {
    render_catalog_scoped_section(
        renderer,
        "Tasks",
        display_contexts,
        |_| Ok(()),
        |renderer| {
            render_ordered_catalog_task_rows(renderer, color_enabled, theme, display_contexts)
        },
    )
}

fn render_catalog_scoped_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    title: &str,
    display_contexts: &[CatalogManifestContext<'_>],
    render_header: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
    render_rows: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<bool, RunnerError>,
) -> Result<(), RunnerError> {
    renderer.section(title)?;
    render_header(renderer)?;
    let has_rows = if display_contexts.is_empty() {
        false
    } else {
        render_rows(renderer)?
    };
    if !has_rows {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;
    Ok(())
}

fn render_info_notices(
    renderer: &mut PlainRenderer<Vec<u8>>,
    notices: &[String],
) -> Result<(), RunnerError> {
    for notice in notices {
        renderer.notice(NoticeLevel::Info, notice)?;
    }
    Ok(())
}
