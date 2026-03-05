#[path = "json_output/payload.rs"]
mod payload;
#[path = "json_output/row_collector.rs"]
mod row_collector;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, LoadedCatalog, RunnerError};
use super::filtering::FilteredTaskModel;
use super::prepared_listing::{prepare_listing_selection, PreparedListingSelection};
use super::render_context::ListingRenderRequest;
use super::ListingCatalogSnapshot;
use payload::{encode_catalog_payload, encode_filtered_payload, JsonPayloadContext};
use row_collector::{collect_all_catalog_rows, collect_filtered_rows};
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
        PreparedListingSelection::Filtered {
            filter,
            filtered_model,
        } => build_filtered_payload(&json_context, filter, filtered_model),
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
    let rows = collect_all_catalog_rows(ordered_catalogs);
    encode_catalog_payload(context, rows, builtin_task_rows_json())
}

fn build_filtered_payload(
    context: &JsonPayloadContext<'_>,
    filter: &str,
    filtered_model: FilteredTaskModel<'_>,
) -> Result<serde_json::Value, RunnerError> {
    let rows = collect_filtered_rows(filtered_model.catalog_matches(), filtered_model.task_name());
    encode_filtered_payload(
        context,
        filter,
        rows,
        builtin_rows_json(filtered_model.builtin_matches().iter().copied()),
        filtered_model.into_notes(),
    )
}
