use super::super::super::{LoadedCatalog, RunnerError};
use super::super::filtering::evaluate_task_filter;
use super::payload::{encode_tasks_payload, JsonPayloadBody, JsonPayloadContext};
use super::row_collector::{collect_all_catalog_rows, collect_filtered_rows};
use super::rows::{builtin_rows_json, builtin_task_rows_json};

pub(super) enum JsonPayloadSelection<'a> {
    Catalog {
        ordered_catalogs: &'a [&'a LoadedCatalog],
    },
    Filtered {
        catalogs: &'a [LoadedCatalog],
        filter: &'a str,
    },
}

pub(super) fn build_tasks_payload(
    context: &JsonPayloadContext<'_>,
    selection: JsonPayloadSelection<'_>,
) -> Result<serde_json::Value, RunnerError> {
    match selection {
        JsonPayloadSelection::Catalog { ordered_catalogs } => {
            let rows = collect_all_catalog_rows(ordered_catalogs);
            encode_tasks_payload(
                context,
                JsonPayloadBody::Catalog {
                    rows,
                    builtin_tasks: builtin_task_rows_json(),
                },
            )
        }
        JsonPayloadSelection::Filtered { catalogs, filter } => {
            let (task_name, catalog_matches, builtin_matches, notes) =
                evaluate_task_filter(catalogs, filter)?.into_render_parts();
            let rows = collect_filtered_rows(catalog_matches.as_slice(), task_name.as_str());
            encode_tasks_payload(
                context,
                JsonPayloadBody::Filtered {
                    filter,
                    rows,
                    builtin_matches: builtin_rows_json(builtin_matches.iter().copied()),
                    notes,
                },
            )
        }
    }
}
