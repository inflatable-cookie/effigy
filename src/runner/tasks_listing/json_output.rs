#[path = "json_output/payload.rs"]
mod payload;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, LoadedCatalog, RunnerError};
use super::prepared_task_rows::{prepare_all_catalog_rows_json, prepare_filtered_rows_json};
use super::render_context::ListingRenderRequest;
use super::selection::PreparedFilteredListing;
use super::selection_dispatch::dispatch_listing_selection;
use super::ListingCatalogSnapshot;
use payload::{encode_catalog_payload, encode_filtered_payload, JsonPayloadContext};
use rows::{builtin_rows_json, builtin_task_rows_json};

struct JsonDispatchContext<'a, 'snap> {
    json_context: &'a JsonPayloadContext<'snap>,
}

pub(super) fn render_tasks_json(
    request: ListingRenderRequest<'_>,
    snapshot: &ListingCatalogSnapshot<'_>,
) -> Result<String, RunnerError> {
    let json_context = JsonPayloadContext::new(
        snapshot.catalogs().len(),
        snapshot.catalog_diagnostics(),
        snapshot.precedence(),
        request.resolve_probe(),
    );
    let mut context = JsonDispatchContext {
        json_context: &json_context,
    };
    let payload = dispatch_listing_selection(
        request,
        snapshot,
        &mut context,
        render_catalog_payload,
        render_filtered_payload,
    )?;
    render::encode_json(&payload, request.pretty_json())
}

fn render_catalog_payload(
    context: &mut JsonDispatchContext<'_, '_>,
    ordered_catalogs: &[&LoadedCatalog],
) -> Result<serde_json::Value, RunnerError> {
    build_catalog_payload(context.json_context, ordered_catalogs)
}

fn render_filtered_payload(
    context: &mut JsonDispatchContext<'_, '_>,
    filtered_listing: PreparedFilteredListing<'_>,
) -> Result<serde_json::Value, RunnerError> {
    build_filtered_payload(context.json_context, filtered_listing)
}

fn build_catalog_payload(
    context: &JsonPayloadContext<'_>,
    ordered_catalogs: &[&LoadedCatalog],
) -> Result<serde_json::Value, RunnerError> {
    let rows = prepare_all_catalog_rows_json(ordered_catalogs);
    encode_catalog_payload(context, rows, builtin_task_rows_json())
}

fn build_filtered_payload(
    context: &JsonPayloadContext<'_>,
    filtered_listing: PreparedFilteredListing<'_>,
) -> Result<serde_json::Value, RunnerError> {
    let rows = prepare_filtered_rows_json(
        filtered_listing.catalog_matches(),
        filtered_listing.task_name(),
    );
    let builtin_matches = builtin_rows_json(filtered_listing.builtin_matches().iter().copied());
    let (filter, notes) = filtered_listing.into_filter_and_notes();
    encode_filtered_payload(context, filter, rows, builtin_matches, notes)
}
