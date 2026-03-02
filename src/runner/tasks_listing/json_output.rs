#[path = "json_output/catalog_payload.rs"]
mod catalog_payload;
#[path = "json_output/filtered_payload.rs"]
mod filtered_payload;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, LoadedCatalog, RunnerError};
use super::render_context::ListingRenderContext;
use catalog_payload::build_catalog_payload;
use filtered_payload::build_filtered_tasks_payload;

pub(super) fn render_tasks_json(
    context: &ListingRenderContext<'_>,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
) -> Result<String, RunnerError> {
    if let Some(filter) = context.filter() {
        let payload = build_filtered_tasks_payload(
            catalogs,
            catalog_diagnostics,
            precedence,
            context.resolve_probe(),
            filter,
        )?;
        return render::encode_json(&payload, context.pretty_json());
    }

    let payload = build_catalog_payload(
        catalogs,
        ordered_catalogs,
        catalog_diagnostics,
        precedence,
        context.resolve_probe(),
    );
    render::encode_json(&payload, context.pretty_json())
}
