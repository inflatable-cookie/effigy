use crate::ui::theme::Theme;
use crate::ui::{PlainRenderer, Renderer};

use super::super::super::tasks_view::style_text;
use super::super::super::RunnerError;
use super::super::row_projection::{
    project_builtin_rows, BuiltinTaskRow, ProjectedCatalogTaskSignatureRow,
};
use super::model::{PreparedCatalogAliasRow, PreparedCatalogTaskRow};

pub(super) fn render_catalog_alias_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[PreparedCatalogAliasRow],
) -> Result<(), RunnerError> {
    for row in rows {
        render_name_detail_row(renderer, color_enabled, theme, row.alias(), row.manifest())?;
    }
    Ok(())
}

pub(super) fn render_ordered_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[PreparedCatalogTaskRow],
) -> Result<(), RunnerError> {
    render_prepared_catalog_task_rows(renderer, color_enabled, theme, rows)
}

pub(super) fn render_catalog_match_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[PreparedCatalogTaskRow],
) -> Result<(), RunnerError> {
    render_prepared_catalog_task_rows(renderer, color_enabled, theme, rows)
}

pub(super) fn render_builtin_task_rows<'a>(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: impl IntoIterator<Item = BuiltinTaskRow<'a>>,
) -> Result<(), RunnerError> {
    for row in project_builtin_rows(rows) {
        render_name_detail_row(
            renderer,
            color_enabled,
            theme,
            row.task(),
            row.description(),
        )?;
    }
    Ok(())
}

fn render_prepared_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[PreparedCatalogTaskRow],
) -> Result<(), RunnerError> {
    for row in rows {
        render_task_signature_rows(
            renderer,
            color_enabled,
            theme,
            row.manifest(),
            row.signature_rows().iter(),
        )?;
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

fn render_task_signature_rows<'a>(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    rows: impl IntoIterator<Item = &'a ProjectedCatalogTaskSignatureRow>,
) -> Result<(), RunnerError> {
    for row in rows {
        render_task_text_row(
            renderer,
            color_enabled,
            theme,
            manifest,
            row.task(),
            row.run(),
        )?;
    }
    Ok(())
}
