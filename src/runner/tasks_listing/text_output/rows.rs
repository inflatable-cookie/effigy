use crate::ui::theme::Theme;
use crate::ui::{PlainRenderer, Renderer};

use super::super::super::tasks_view::style_text;
use super::super::super::{LoadedCatalog, ManifestTask, RunnerError};
use super::super::row_projection::{project_managed_profiles, project_task_run};

pub(super) fn render_task_with_profiles(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Result<(), RunnerError> {
    let projection = project_task_run(catalog, task_name, task);
    render_task_text_row(
        renderer,
        color_enabled,
        theme,
        manifest,
        &projection.task,
        &projection.run,
    )?;
    for row in project_managed_profiles(catalog, task_name, task) {
        render_task_text_row(
            renderer,
            color_enabled,
            theme,
            manifest,
            &row.task,
            &row.run,
        )?;
    }
    Ok(())
}

pub(super) fn render_builtin_task_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    rows: &[(&str, &str)],
) -> Result<(), RunnerError> {
    for (name, description) in rows {
        renderer.text(&format!(
            "- {} : {}",
            style_text(color_enabled, theme.task_name, name),
            style_text(color_enabled, theme.muted, description),
        ))?;
    }
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
    renderer.text(&format!(
        "- {} : {}",
        style_text(color_enabled, theme.task_name, task_label),
        style_text(color_enabled, theme.muted, manifest),
    ))?;
    renderer.text(&format!(
        "      {}",
        style_text(color_enabled, theme.task_signature, signature),
    ))?;
    Ok(())
}
