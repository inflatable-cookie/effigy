use crate::TasksArgs;

#[path = "json_output/catalog_payload.rs"]
mod catalog_payload;
#[path = "json_output/filtered_payload.rs"]
mod filtered_payload;
#[path = "json_output/rows.rs"]
mod rows;

use super::super::{render, LoadedCatalog, RunnerError};
use catalog_payload::build_catalog_payload;
use filtered_payload::build_filtered_tasks_payload;

pub(super) fn render_tasks_json(
    args: &TasksArgs,
    catalogs: &[LoadedCatalog],
    ordered_catalogs: &[&LoadedCatalog],
    catalog_diagnostics: &[serde_json::Value],
    precedence: &[String],
    resolve_probe: &Option<serde_json::Value>,
) -> Result<String, RunnerError> {
    if let Some(filter) = args.task_name.as_ref() {
        let payload = build_filtered_tasks_payload(
            catalogs,
            catalog_diagnostics,
            precedence,
            resolve_probe,
            filter,
        )?;
        return render::encode_json(&payload, args.pretty_json);
    }

    let payload = build_catalog_payload(
        catalogs,
        ordered_catalogs,
        catalog_diagnostics,
        precedence,
        resolve_probe,
    );
    render::encode_json(&payload, args.pretty_json)
}
