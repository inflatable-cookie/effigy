use std::io::IsTerminal;
use std::path::Path;

use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{OutputMode, PlainRenderer};
use crate::TasksArgs;

#[path = "text_output/filtered.rs"]
mod filtered;
#[path = "text_output/rows.rs"]
mod rows;
#[path = "text_output/sections.rs"]
mod sections;

use super::super::tasks_view::render_resolution_probe_block;
use super::super::{render, LoadedCatalog, RunnerError};
use filtered::render_filtered_tasks_text;
use sections::render_default_tasks_text;

pub(super) fn render_tasks_text(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    let theme = Theme::default();
    if let Some(filter) = args.task_name.as_ref() {
        render_filtered_tasks_text(
            &mut renderer,
            color_enabled,
            &theme,
            catalogs,
            filter,
            resolve_probe,
            resolved_root,
        )?;
        return render::render_utf8(renderer.into_inner());
    }

    if let Some(probe) = resolve_probe {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
        return render::render_utf8(renderer.into_inner());
    }

    render_default_tasks_text(
        &mut renderer,
        color_enabled,
        &theme,
        catalogs,
        ordered_catalogs,
        resolved_root,
    )?;
    render::render_utf8(renderer.into_inner())
}
