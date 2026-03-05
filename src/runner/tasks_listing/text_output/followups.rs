use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::tasks_view::render_resolution_probe_block;
use super::super::super::RunnerError;
use super::super::row_projection::BuiltinTaskProjection;
use super::rows::render_builtin_task_rows;

pub(super) fn render_builtin_rows_section<'a>(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    title: &str,
    rows: impl IntoIterator<Item = BuiltinTaskProjection<'a>>,
    notes: &[String],
) -> Result<(), RunnerError> {
    renderer.section(title)?;
    render_builtin_task_rows(renderer, color_enabled, theme, rows)?;
    render_info_notices(renderer, notes)
}

pub(super) fn render_builtin_and_probe_followup_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    builtin_title: &str,
    builtin_rows: &[BuiltinTaskProjection<'static>],
    notes: &[String],
    resolve_probe: Option<&serde_json::Value>,
) -> Result<(), RunnerError> {
    if builtin_rows.is_empty() && resolve_probe.is_none() {
        return Ok(());
    }

    renderer.text("")?;

    if !builtin_rows.is_empty() {
        render_builtin_rows_section(
            renderer,
            color_enabled,
            theme,
            builtin_title,
            builtin_rows.iter().copied(),
            notes,
        )?;
    }

    if let Some(probe) = resolve_probe {
        if !builtin_rows.is_empty() {
            renderer.text("")?;
        }
        render_resolution_probe_block(renderer, probe, color_enabled, false)?;
    }

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
