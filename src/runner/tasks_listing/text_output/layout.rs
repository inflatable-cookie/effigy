use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::RunnerError;

pub(super) fn render_section_with_optional_rows(
    renderer: &mut PlainRenderer<Vec<u8>>,
    title: &str,
    has_scope_rows: bool,
    render_header: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
    render_rows: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<bool, RunnerError>,
) -> Result<(), RunnerError> {
    renderer.section(title)?;
    render_header(renderer)?;
    let has_rows = render_rows_if_visible(renderer, has_scope_rows, render_rows)?;

    if !has_rows {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;
    Ok(())
}

pub(super) fn render_followup_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    has_primary: bool,
    render_primary: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
    has_secondary: bool,
    render_secondary: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if !has_primary && !has_secondary {
        return Ok(());
    }

    renderer.text("")?;
    render_optional_followup(renderer, has_primary, render_primary)?;
    render_spacer_between_followups(renderer, has_primary && has_secondary)?;
    render_optional_followup(renderer, has_secondary, render_secondary)?;
    Ok(())
}

fn render_rows_if_visible(
    renderer: &mut PlainRenderer<Vec<u8>>,
    has_rows: bool,
    render_rows: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<bool, RunnerError>,
) -> Result<bool, RunnerError> {
    if !has_rows {
        return Ok(false);
    }
    render_rows(renderer)
}

fn render_optional_followup(
    renderer: &mut PlainRenderer<Vec<u8>>,
    should_render: bool,
    render_followup: impl FnOnce(&mut PlainRenderer<Vec<u8>>) -> Result<(), RunnerError>,
) -> Result<(), RunnerError> {
    if should_render {
        render_followup(renderer)?;
    }
    Ok(())
}

fn render_spacer_between_followups(
    renderer: &mut PlainRenderer<Vec<u8>>,
    should_render: bool,
) -> Result<(), RunnerError> {
    if should_render {
        renderer.text("")?;
    }
    Ok(())
}
