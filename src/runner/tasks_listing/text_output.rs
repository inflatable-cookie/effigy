use effigy_ui::theme::Theme;
use effigy_ui::{plain_renderer, render_utf8, text_color_enabled, PlainRenderer};

#[path = "text_output/filtered.rs"]
mod filtered;
#[path = "text_output/followups.rs"]
mod followups;
#[path = "text_output/rows.rs"]
mod rows;
#[path = "text_output/sections.rs"]
mod sections;

use super::super::tasks_view::render_resolution_probe_block;
use super::super::RunnerError;
use super::render_context::ListingRenderRequest;
use super::selection::PreparedFilteredListing;
use super::selection_dispatch::dispatch_listing_selection;
use super::ListingCatalogSnapshot;
use effigy_manifest::LoadedCatalog;
use filtered::render_filtered_tasks_text;
use sections::render_default_tasks_text;

struct TextDispatchContext<'a> {
    renderer: &'a mut PlainRenderer<Vec<u8>>,
    color_enabled: bool,
    theme: &'a Theme,
    catalogs: &'a [LoadedCatalog],
    resolve_probe: &'a Option<serde_json::Value>,
    resolved_root: &'a std::path::Path,
}

pub(super) fn render_tasks_text(
    request: ListingRenderRequest<'_>,
    snapshot: &ListingCatalogSnapshot<'_>,
) -> Result<String, RunnerError> {
    let color_enabled = text_color_enabled();
    let mut renderer = plain_renderer(color_enabled);
    let theme = Theme::default();
    let resolve_probe = request.resolve_probe();

    if let Some(probe) = request.resolve_only_probe() {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
    } else {
        let mut context = TextDispatchContext {
            renderer: &mut renderer,
            color_enabled,
            theme: &theme,
            catalogs: snapshot.catalogs(),
            resolve_probe,
            resolved_root: snapshot.resolved_root(),
        };
        dispatch_listing_selection(
            request,
            snapshot,
            &mut context,
            render_catalog_selection,
            render_filtered_selection,
        )?;
    }

    Ok(render_utf8(renderer.into_inner())?)
}

fn render_catalog_selection(
    context: &mut TextDispatchContext<'_>,
    ordered_catalogs: &[&LoadedCatalog],
) -> Result<(), RunnerError> {
    render_default_tasks_text(
        context.renderer,
        context.color_enabled,
        context.theme,
        context.catalogs,
        ordered_catalogs,
        context.resolved_root,
    )
}

fn render_filtered_selection(
    context: &mut TextDispatchContext<'_>,
    filtered_listing: PreparedFilteredListing<'_>,
) -> Result<(), RunnerError> {
    render_filtered_tasks_text(
        context.renderer,
        context.color_enabled,
        context.theme,
        &filtered_listing,
        context.resolve_probe,
        context.resolved_root,
    )
}
