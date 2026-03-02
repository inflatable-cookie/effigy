use std::path::Path;

use crate::TasksArgs;

#[path = "tasks_listing/catalog_rows.rs"]
mod catalog_rows;
#[path = "tasks_listing/filtering.rs"]
mod filtering;
#[path = "tasks_listing/json_output.rs"]
mod json_output;
#[path = "tasks_listing/matches.rs"]
mod matches;
#[path = "tasks_listing/render_context.rs"]
mod render_context;
#[path = "tasks_listing/text_output.rs"]
mod text_output;

use super::{LoadedCatalog, RunnerError};
use render_context::ListingRenderContext;

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
    let context = ListingRenderContext::new(args, resolve_probe);
    if args.output_json {
        return json_output::render_tasks_json(
            &context,
            catalogs,
            ordered_catalogs,
            catalog_diagnostics,
            precedence,
        );
    }

    text_output::render_tasks_text(&context, catalogs, ordered_catalogs, resolved_root)
}

fn manifest_path_string(catalog: &LoadedCatalog) -> String {
    catalog.manifest_path.display().to_string()
}
