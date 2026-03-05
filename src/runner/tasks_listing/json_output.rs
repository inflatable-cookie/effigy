#[path = "json_output/assembly.rs"]
mod assembly;
#[path = "json_output/payload.rs"]
mod payload;
#[path = "json_output/row_collector.rs"]
mod row_collector;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, RunnerError};
use super::render_context::ListingRenderRequest;
use super::ListingCatalogSnapshot;
use assembly::{build_tasks_payload, JsonPayloadSelection};
use payload::JsonPayloadContext;

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

    let payload = request.dispatch_selection(
        |filter| {
            build_tasks_payload(
                &json_context,
                JsonPayloadSelection::Filtered {
                    catalogs: snapshot.catalogs(),
                    filter,
                },
            )
        },
        || {
            build_tasks_payload(
                &json_context,
                JsonPayloadSelection::Catalog {
                    ordered_catalogs: snapshot.ordered_catalogs(),
                },
            )
        },
    )?;
    render::encode_json(&payload, request.pretty_json())
}
