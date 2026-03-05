use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use super::super::super::{LoadedCatalog, RunnerError};
use super::super::row_projection::{builtin_task_rows, BuiltinTaskRow};
use super::model::{prepare_default_text_rows, PreparedCatalogAliasRow, PreparedCatalogTaskRow};
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
    let prepared_rows = prepare_default_text_rows(ordered_catalogs, resolved_root);
    let has_catalog_scope = !prepared_rows.catalog_alias_rows().is_empty();
    render_catalogs_section(
        renderer,
        color_enabled,
        theme,
        catalogs,
        has_catalog_scope,
        prepared_rows.catalog_alias_rows(),
    )?;
    render_tasks_section(
        renderer,
        color_enabled,
        theme,
        has_catalog_scope,
        prepared_rows.catalog_task_rows(),
    )?;
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
    has_scope_rows: bool,
    alias_rows: &[PreparedCatalogAliasRow],
) -> Result<(), RunnerError> {
    render_catalog_scoped_section(
        renderer,
        "Catalogs",
        has_scope_rows,
        !alias_rows.is_empty(),
        |renderer| {
            renderer.key_values(&[KeyValue::new("count", catalogs.len().to_string())])?;
            Ok(())
        },
        |renderer| render_catalog_alias_rows(renderer, color_enabled, theme, alias_rows),
    )
}

fn render_tasks_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    has_scope_rows: bool,
    task_rows: &[PreparedCatalogTaskRow],
) -> Result<(), RunnerError> {
    render_catalog_scoped_section(
        renderer,
        "Tasks",
        has_scope_rows,
        !task_rows.is_empty(),
        |_| Ok(()),
        |renderer| render_ordered_catalog_task_rows(renderer, color_enabled, theme, task_rows),
    )
}

fn render_catalog_scoped_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    title: &str,
    has_scope_rows: bool,
    has_rows: bool,
    render_header: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
    render_rows: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    renderer.section(title)?;
    render_header(renderer)?;
    if has_scope_rows && has_rows {
        render_rows(renderer)?;
    } else {
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
