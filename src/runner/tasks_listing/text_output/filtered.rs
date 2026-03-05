use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::tasks_view::render_resolution_probe_block;
use super::super::super::RunnerError;
use super::super::row_projection::BuiltinTaskRow;
use super::super::selection::PreparedFilteredListing;
use super::model::prepare_catalog_match_task_rows;
use super::rows::render_catalog_match_rows;
use super::sections::render_builtin_rows_section;

pub(super) fn render_filtered_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    filtered_listing: &PreparedFilteredListing<'_>,
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    renderer.section(&format!("Task Matches: {}", filtered_listing.filter()))?;

    if !filtered_listing.has_matches() {
        renderer.notice(NoticeLevel::Warning, "no matches")?;
        return Ok(());
    }

    let matched_rows = prepare_catalog_match_task_rows(
        filtered_listing.catalog_matches(),
        filtered_listing.task_name(),
        resolved_root,
    );
    render_catalog_match_rows(renderer, color_enabled, theme, matched_rows.as_slice())?;
    render_filtered_followup_sections(
        renderer,
        color_enabled,
        theme,
        filtered_listing.builtin_matches(),
        filtered_listing.notes(),
        resolve_probe,
    )?;
    Ok(())
}

fn render_filtered_followup_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    builtin_matches: &[BuiltinTaskRow<'static>],
    notes: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> Result<(), RunnerError> {
    if builtin_matches.is_empty() && resolve_probe.is_none() {
        return Ok(());
    }

    renderer.text("")?;

    if !builtin_matches.is_empty() {
        render_builtin_rows_section(
            renderer,
            color_enabled,
            theme,
            "Built-in Task Matches",
            builtin_matches.iter().copied(),
            notes,
        )?;
    }

    if let Some(probe) = resolve_probe {
        if !builtin_matches.is_empty() {
            renderer.text("")?;
        }
        render_resolution_probe_block(renderer, probe, color_enabled, false)?;
    }

    Ok(())
}
