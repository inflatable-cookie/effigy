use crate::ui::theme::Theme;
use crate::ui::{PlainRenderer, Renderer};

use super::super::super::tasks_view::{style_text, ManagedProfileDisplayRow};
use super::super::super::RunnerError;

pub(super) fn render_task_with_profiles(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    manifest: &str,
    task_label: &str,
    signature: &str,
    managed_profiles: Vec<ManagedProfileDisplayRow>,
) -> Result<(), RunnerError> {
    render_task_text_row(
        renderer,
        color_enabled,
        theme,
        manifest,
        task_label,
        signature,
    )?;
    for row in managed_profiles {
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
