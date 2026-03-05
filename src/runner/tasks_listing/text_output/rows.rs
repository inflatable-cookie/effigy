use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{PlainRenderer, Renderer};

use super::super::super::tasks_view::style_text;
use super::super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::super::catalog_iteration::catalog_tasks;
use super::super::catalog_manifest::{manifest_display_context, CatalogManifestContext};
use super::super::row_projection::{project_builtin_rows, project_catalog_task_display_rows};

pub(super) fn render_catalog_alias_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalog_contexts: &[CatalogManifestContext<'_>],
) -> Result<bool, RunnerError> {
    for context in catalog_contexts {
        render_name_detail_row(
            renderer,
            color_enabled,
            theme,
            &context.catalog().alias,
            context.manifest(),
        )?;
    }
    Ok(!catalog_contexts.is_empty())
}

pub(super) fn render_ordered_catalog_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalog_contexts: &[CatalogManifestContext<'_>],
) -> Result<bool, RunnerError> {
    let mut rendered_any = false;
    for context in catalog_contexts {
        for (task_name, task) in catalog_tasks(context.catalog()) {
            render_catalog_task_with_profiles(
                renderer,
                color_enabled,
                theme,
                context.catalog(),
                task_name,
                task,
                context.manifest(),
            )?;
            rendered_any = true;
        }
    }
    Ok(rendered_any)
}

pub(super) fn render_catalog_match_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    resolved_root: &Path,
    task_name: &str,
    matches: &[(&LoadedCatalog, &ManifestTask)],
) -> Result<bool, RunnerError> {
    let mut rendered_any = false;
    for (catalog, task) in matches {
        let context = manifest_display_context(catalog, resolved_root);
        render_catalog_task_with_profiles(
            renderer,
            color_enabled,
            theme,
            catalog,
            task_name,
            task,
            context.manifest(),
        )?;
        rendered_any = true;
    }
    Ok(rendered_any)
}

fn render_catalog_task_with_profiles(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
    manifest: &str,
) -> Result<(), RunnerError> {
    let signature_rows =
        project_catalog_task_display_rows(catalog, task_name, task).into_signature_rows();
    render_task_signature_rows(renderer, color_enabled, theme, manifest, signature_rows)
}

pub(super) fn render_builtin_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[(&str, &str)],
) -> Result<(), RunnerError> {
    for (task, description) in project_builtin_rows(rows.iter().copied()) {
        render_name_detail_row(renderer, color_enabled, theme, task, description)?;
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

fn render_task_signature_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    rows: impl IntoIterator<Item = (String, String)>,
) -> Result<(), RunnerError> {
    for (task_label, signature) in rows {
        render_task_text_row(
            renderer,
            color_enabled,
            theme,
            manifest,
            &task_label,
            &signature,
        )?;
    }
    Ok(())
}
