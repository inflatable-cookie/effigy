use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer};

use super::super::super::model::catalog::LoadedCatalog;
use super::super::prepared_task_rows::{
    prepare_default_text_rows, CatalogAliasProjection, CatalogTaskProjection,
};
use super::super::row_projection::builtin_task_rows;
use super::followups::render_builtin_rows_section;
use super::rows::{render_catalog_alias_rows, render_catalog_task_rows};
use crate::runner::error::RunnerError;

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

fn render_catalogs_section(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    has_scope_rows: bool,
    alias_rows: &[CatalogAliasProjection],
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
    task_rows: &[CatalogTaskProjection],
) -> Result<(), RunnerError> {
    render_catalog_scoped_section(
        renderer,
        "Tasks",
        has_scope_rows,
        !task_rows.is_empty(),
        |_| Ok(()),
        |renderer| render_catalog_task_rows(renderer, color_enabled, theme, task_rows),
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
