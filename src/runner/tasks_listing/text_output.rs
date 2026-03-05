use crate::ui::theme::Theme;

#[path = "text_output/filtered.rs"]
mod filtered;
#[path = "text_output/layout.rs"]
mod layout;
#[path = "text_output/rows.rs"]
mod rows;
#[path = "text_output/sections.rs"]
mod sections;

use super::super::tasks_view::render_resolution_probe_block;
use super::super::{render, RunnerError};
use super::render_context::{ListingRenderRequest, TextRenderDispatch};
use super::ListingCatalogSnapshot;
use filtered::render_filtered_tasks_text;
use sections::render_default_tasks_text;

pub(super) fn render_tasks_text(
    request: ListingRenderRequest<'_>,
    snapshot: &ListingCatalogSnapshot<'_>,
) -> Result<String, RunnerError> {
    let color_enabled = render::text_color_enabled();
    let mut renderer = render::plain_renderer(color_enabled);
    let theme = Theme::default();

    match request.text_dispatch() {
        TextRenderDispatch::ResolveOnly(probe) => {
            render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?
        }
        TextRenderDispatch::Filtered(filter) => render_filtered_tasks_text(
            &mut renderer,
            color_enabled,
            &theme,
            snapshot.catalogs(),
            filter,
            request.resolve_probe(),
            snapshot.resolved_root(),
        )?,
        TextRenderDispatch::Catalog => render_default_tasks_text(
            &mut renderer,
            color_enabled,
            &theme,
            snapshot.catalogs(),
            snapshot.ordered_catalogs(),
            snapshot.resolved_root(),
        )?,
    }

    render::render_utf8(renderer.into_inner())
}
