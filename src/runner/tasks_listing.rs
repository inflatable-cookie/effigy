use std::path::Path;

use crate::TasksArgs;

#[path = "tasks_listing/catalog_iteration.rs"]
mod catalog_iteration;
#[path = "tasks_listing/catalog_manifest.rs"]
mod catalog_manifest;
#[path = "tasks_listing/json_output.rs"]
mod json_output;
#[path = "tasks_listing/prepared_task_rows.rs"]
mod prepared_task_rows;
#[path = "tasks_listing/render_context.rs"]
mod render_context;
#[path = "tasks_listing/row_projection.rs"]
mod row_projection;
#[path = "tasks_listing/selection.rs"]
mod selection;
#[path = "tasks_listing/selection_dispatch.rs"]
mod selection_dispatch;
#[path = "tasks_listing/snapshot.rs"]
mod snapshot;
#[path = "tasks_listing/text_output.rs"]
mod text_output;

use super::model::catalog::LoadedCatalog;
use crate::runner::error::RunnerError;
use render_context::ListingRenderRequest;
pub(super) use snapshot::{build_catalog_diagnostics, ListingCatalogSnapshot};

const BUILTIN_TEST_FALLBACK_NOTE: &str =
    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined";

pub(super) fn render_tasks_listing(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
    resolved_root: &Path,
) -> Result<String, RunnerError> {
    let request = ListingRenderRequest::from_args(args, resolve_probe);
    let snapshot = ListingCatalogSnapshot::new(
        catalogs,
        ordered_catalogs,
        catalog_diagnostics,
        precedence,
        resolved_root,
    );
    if request.output_json() {
        json_output::render_tasks_json(request, &snapshot)
    } else {
        text_output::render_tasks_text(request, &snapshot)
    }
}
