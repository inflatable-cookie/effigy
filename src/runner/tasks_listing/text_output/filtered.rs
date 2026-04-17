use std::path::Path;

use effigy_ui::theme::Theme;
use effigy_ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::prepared_task_rows::prepare_catalog_match_task_rows;
use super::super::selection::PreparedFilteredListing;
use super::followups::render_builtin_and_probe_followup_sections;
use super::rows::render_catalog_task_rows;
use crate::runner::error::RunnerError;

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
        render_builtin_and_probe_followup_sections(
            renderer,
            color_enabled,
            theme,
            "Built-in Task Matches",
            filtered_listing.builtin_matches(),
            filtered_listing.notes(),
            resolve_probe.as_ref(),
        )?;
        return Ok(());
    }

    let matched_rows = prepare_catalog_match_task_rows(
        filtered_listing.catalog_matches(),
        filtered_listing.task_name(),
        resolved_root,
    );
    render_catalog_task_rows(renderer, color_enabled, theme, matched_rows.as_slice())?;
    render_builtin_and_probe_followup_sections(
        renderer,
        color_enabled,
        theme,
        "Built-in Task Matches",
        filtered_listing.builtin_matches(),
        filtered_listing.notes(),
        resolve_probe.as_ref(),
    )?;
    Ok(())
}
