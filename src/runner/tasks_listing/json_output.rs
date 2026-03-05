#[path = "json_output/model.rs"]
mod model;
#[path = "json_output/payload.rs"]
mod payload;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, LoadedCatalog, RunnerError};
use super::render_context::ListingRenderRequest;
use super::selection::{
    prepare_listing_selection, PreparedFilteredListing, PreparedListingSelection,
};
use super::ListingCatalogSnapshot;
use model::{prepare_all_catalog_rows_json, prepare_filtered_rows_json};
use payload::{encode_catalog_payload, encode_filtered_payload, JsonPayloadContext};
use rows::{builtin_rows_json, builtin_task_rows_json};

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

    let payload = match prepare_listing_selection(request, snapshot)? {
        PreparedListingSelection::Filtered { filtered_listing } => {
            build_filtered_payload(&json_context, filtered_listing)
        }
        PreparedListingSelection::Catalog { ordered_catalogs } => {
            build_catalog_payload(&json_context, ordered_catalogs)
        }
    }?;
    render::encode_json(&payload, request.pretty_json())
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
    let filter = filtered_listing.filter().to_owned();
    let rows = prepare_filtered_rows_json(
        filtered_listing.catalog_matches(),
        filtered_listing.task_name(),
    );
    let builtin_matches = builtin_rows_json(filtered_listing.builtin_matches().iter().copied());
    let notes = filtered_listing.into_notes();
    encode_filtered_payload(context, &filter, rows, builtin_matches, notes)
}
