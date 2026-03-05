use std::path::Path;

use crate::TasksArgs;

#[path = "tasks_listing/builtin_rows.rs"]
mod builtin_rows;
#[path = "tasks_listing/catalog_iteration.rs"]
mod catalog_iteration;
#[path = "tasks_listing/catalog_manifest.rs"]
mod catalog_manifest;
#[path = "tasks_listing/filtering.rs"]
mod filtering;
#[path = "tasks_listing/json_output.rs"]
mod json_output;
#[path = "tasks_listing/matches.rs"]
mod matches;
#[path = "tasks_listing/prepared_listing.rs"]
mod prepared_listing;
#[path = "tasks_listing/prepared_task_rows.rs"]
mod prepared_task_rows;
#[path = "tasks_listing/render_context.rs"]
mod render_context;
#[path = "tasks_listing/row_projection.rs"]
mod row_projection;
#[path = "tasks_listing/text_output.rs"]
mod text_output;

use super::{LoadedCatalog, RunnerError};
use render_context::ListingRenderRequest;

const BUILTIN_TEST_FALLBACK_NOTE: &str =
    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined";

pub(super) struct ListingCatalogSnapshot<'a> {
    catalogs: &'a [LoadedCatalog],
    ordered_catalogs: &'a [&'a LoadedCatalog],
    catalog_diagnostics: &'a [serde_json::Value],
    precedence: &'a [String],
    resolved_root: &'a Path,
}

impl<'a> ListingCatalogSnapshot<'a> {
    fn new(
        catalogs: &'a [LoadedCatalog],
        ordered_catalogs: &'a [&'a LoadedCatalog],
        catalog_diagnostics: &'a [serde_json::Value],
        precedence: &'a [String],
        resolved_root: &'a Path,
    ) -> Self {
        Self {
            catalogs,
            ordered_catalogs,
            catalog_diagnostics,
            precedence,
            resolved_root,
        }
    }

    pub(super) fn catalogs(&self) -> &'a [LoadedCatalog] {
        self.catalogs
    }

    pub(super) fn ordered_catalogs(&self) -> &'a [&'a LoadedCatalog] {
        self.ordered_catalogs
    }

    pub(super) fn catalog_diagnostics(&self) -> &'a [serde_json::Value] {
        self.catalog_diagnostics
    }

    pub(super) fn precedence(&self) -> &'a [String] {
        self.precedence
    }

    pub(super) fn resolved_root(&self) -> &'a Path {
        self.resolved_root
    }
}

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
