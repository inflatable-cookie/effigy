use std::path::Path;

use crate::TasksArgs;

#[path = "tasks_listing/filtering.rs"]
mod filtering;
#[path = "tasks_listing/json_output.rs"]
mod json_output;
#[path = "tasks_listing/matches.rs"]
mod matches;
#[path = "tasks_listing/text_output.rs"]
mod text_output;

use super::{LoadedCatalog, RunnerError};

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
    if args.output_json {
        return json_output::render_tasks_json(
            args,
            catalogs,
            ordered_catalogs,
            catalog_diagnostics,
            precedence,
            resolve_probe,
        );
    }

    text_output::render_tasks_text(
        args,
        catalogs,
        ordered_catalogs,
        resolve_probe,
        resolved_root,
    )
}

fn manifest_path_string(catalog: &LoadedCatalog) -> String {
    catalog.manifest_path.display().to_string()
}
