use std::path::Path;

use crate::ui::theme::Theme;
use crate::ui::{NoticeLevel, PlainRenderer, Renderer};

use super::super::super::tasks_view::render_resolution_probe_block;
use super::super::super::{LoadedCatalog, RunnerError};
use super::super::filtering::evaluate_task_filter;
use super::layout::render_followup_sections;
use super::rows::render_catalog_match_rows;
use super::sections::render_builtin_rows_section;

pub(super) fn render_filtered_tasks_text(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    catalogs: &[LoadedCatalog],
    filter: &str,
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<(), RunnerError> {
    let evaluation = evaluate_task_filter(catalogs, filter)?;
    renderer.section(&format!("Task Matches: {filter}"))?;

    if !evaluation.has_matches() {
        renderer.notice(NoticeLevel::Warning, "no matches")?;
        return Ok(());
    }

    let _ = render_catalog_match_rows(
        renderer,
        color_enabled,
        theme,
        resolved_root,
        evaluation.task_name(),
        evaluation.catalog_matches(),
    )?;
    render_filtered_followup_sections(
        renderer,
        color_enabled,
        theme,
        evaluation.builtin_matches(),
        evaluation.notes(),
        resolve_probe,
    )?;
    Ok(())
}

fn render_filtered_followup_sections(
    renderer: &mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &Theme,
    builtin_matches: &[(&str, &str)],
    notes: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> Result<(), RunnerError> {
    render_followup_sections(
        renderer,
        !builtin_matches.is_empty(),
        |renderer| {
            render_builtin_rows_section(
                renderer,
                color_enabled,
                theme,
                "Built-in Task Matches",
                builtin_matches,
                notes,
            )
        },
        resolve_probe.is_some(),
        |renderer| {
            if let Some(probe) = resolve_probe {
                render_resolution_probe_block(renderer, probe, color_enabled, false)?;
            }
            Ok(())
        },
    )
}
