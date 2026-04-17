use effigy_ui::theme::Theme;
use effigy_ui::{PlainRenderer, Renderer};

use super::super::super::tasks_view::style_text;
use super::super::prepared_task_rows::{CatalogAliasProjection, CatalogTaskProjection};
use super::super::row_projection::BuiltinTaskProjection;
use crate::runner::error::RunnerError;

pub(super) fn render_catalog_alias_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[CatalogAliasProjection],
) -> Result<(), RunnerError> {
    for row in rows {
        render_name_detail_row(renderer, color_enabled, theme, row.alias(), row.manifest())?;
    }
    Ok(())
}

pub(super) fn render_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[CatalogTaskProjection],
) -> Result<(), RunnerError> {
    render_prepared_catalog_task_rows(renderer, color_enabled, theme, rows)
}

pub(super) fn render_builtin_task_rows<'a>(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: impl IntoIterator<Item = BuiltinTaskProjection<'a>>,
) -> Result<(), RunnerError> {
    for (task, description) in rows {
        render_name_detail_row(renderer, color_enabled, theme, task, description)?;
    }
    Ok(())
}

fn render_prepared_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[CatalogTaskProjection],
) -> Result<(), RunnerError> {
    for row in rows {
        render_task_text_row(
            renderer,
            color_enabled,
            theme,
            row.manifest(),
            row.task_row().task(),
            row.task_row().run(),
        )?;
        for profile_row in row.managed_profiles() {
            render_task_text_row(
                renderer,
                color_enabled,
                theme,
                row.manifest(),
                profile_row.task.as_str(),
                profile_row.run.as_str(),
            )?;
        }
    }
    Ok(())
}

fn render_name_detail_row(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    name: &str,
    detail: &str,
) -> Result<(), RunnerError> {
    renderer.text(&format!(
        "- {} : {}",
        style_text(color_enabled, theme.task_name, name),
        style_text(color_enabled, theme.muted, detail),
    ))?;
    Ok(())
}

fn render_task_text_row(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    task_label: &str,
    signature: &str,
) -> Result<(), RunnerError> {
    render_name_detail_row(renderer, color_enabled, theme, task_label, manifest)?;
    renderer.text(&format!(
        "      {}",
        style_text(color_enabled, theme.task_signature, signature),
    ))?;
    Ok(())
}
