use serde::Serialize;
use serde_json::Value;

use super::super::prepared_task_rows::{
    CatalogTaskJsonRow, CatalogTaskJsonRows, ManagedProfileJsonRow,
};
use super::rows::BuiltinTaskJsonRow;
use crate::runner::error::RunnerError;

const TASKS_SCHEMA: &str = "effigy.tasks.v1";
const FILTERED_TASKS_SCHEMA: &str = "effigy.tasks.filtered.v1";
const SCHEMA_VERSION: u64 = 1;

pub(super) struct JsonPayloadContext<'a> {
    catalog_count: usize,
    catalog_diagnostics: &'a [Value],
    precedence: &'a [String],
    resolve_probe: &'a Option<Value>,
}

#[derive(Serialize)]
struct PayloadEnvelope<B: Serialize> {
    #[serde(flatten)]
    header: PayloadHeader,
    #[serde(flatten)]
    body: B,
    #[serde(flatten)]
    footer: SharedPayloadFooter,
}

#[derive(Serialize)]
struct CatalogPayloadBody {
    catalog_tasks: Vec<CatalogTaskJsonRow>,
    managed_profiles: Vec<ManagedProfileJsonRow>,
    builtin_tasks: Vec<BuiltinTaskJsonRow>,
}

#[derive(Serialize)]
struct FilteredPayloadBody {
    filter: String,
    matches: Vec<CatalogTaskJsonRow>,
    managed_profile_matches: Vec<ManagedProfileJsonRow>,
    builtin_matches: Vec<BuiltinTaskJsonRow>,
    notes: Vec<String>,
}

#[derive(Serialize)]
struct PayloadHeader {
    schema: &'static str,
    schema_version: u64,
    catalog_count: usize,
}

#[derive(Clone, Serialize)]
struct SharedPayloadFooter {
    catalogs: Vec<Value>,
    precedence: Vec<String>,
    resolve: Option<Value>,
}

struct SplitCatalogTaskJsonRows {
    task_rows: Vec<CatalogTaskJsonRow>,
    managed_profile_rows: Vec<ManagedProfileJsonRow>,
}

impl<'a> JsonPayloadContext<'a> {
    pub(super) fn new(
        catalog_count: usize,
        catalog_diagnostics: &'a [Value],
        precedence: &'a [String],
        resolve_probe: &'a Option<Value>,
    ) -> Self {
        Self {
            catalog_count,
            catalog_diagnostics,
            precedence,
            resolve_probe,
        }
    }

    fn header(&self, schema: &'static str) -> PayloadHeader {
        PayloadHeader {
            schema,
            schema_version: SCHEMA_VERSION,
            catalog_count: self.catalog_count,
        }
    }

    fn footer(&self) -> SharedPayloadFooter {
        SharedPayloadFooter {
            catalogs: self.catalog_diagnostics.to_vec(),
            precedence: self.precedence.to_vec(),
            resolve: self.resolve_probe.clone(),
        }
    }
}

impl From<CatalogTaskJsonRows> for SplitCatalogTaskJsonRows {
    fn from(rows: CatalogTaskJsonRows) -> Self {
        let (task_rows, managed_profile_rows) = rows.into_parts();
        Self {
            task_rows,
            managed_profile_rows,
        }
    }
}

impl SplitCatalogTaskJsonRows {
    fn into_catalog_body(self, builtin_tasks: Vec<BuiltinTaskJsonRow>) -> CatalogPayloadBody {
        CatalogPayloadBody {
            catalog_tasks: self.task_rows,
            managed_profiles: self.managed_profile_rows,
            builtin_tasks,
        }
    }

    fn into_filtered_body(
        self,
        filter: String,
        builtin_matches: Vec<BuiltinTaskJsonRow>,
        notes: Vec<String>,
    ) -> FilteredPayloadBody {
        FilteredPayloadBody {
            filter,
            matches: self.task_rows,
            managed_profile_matches: self.managed_profile_rows,
            builtin_matches,
            notes,
        }
    }
}

pub(super) fn encode_catalog_payload(
    context: &JsonPayloadContext<'_>,
    rows: CatalogTaskJsonRows,
    builtin_tasks: Vec<BuiltinTaskJsonRow>,
) -> Result<Value, RunnerError> {
    let rows = SplitCatalogTaskJsonRows::from(rows);
    encode_payload_with_schema(context, TASKS_SCHEMA, rows.into_catalog_body(builtin_tasks))
}

pub(super) fn encode_filtered_payload(
    context: &JsonPayloadContext<'_>,
    filter: String,
    rows: CatalogTaskJsonRows,
    builtin_matches: Vec<BuiltinTaskJsonRow>,
    notes: Vec<String>,
) -> Result<Value, RunnerError> {
    let rows = SplitCatalogTaskJsonRows::from(rows);
    encode_payload_with_schema(
        context,
        FILTERED_TASKS_SCHEMA,
        rows.into_filtered_body(filter, builtin_matches, notes),
    )
}

fn encode_payload_with_schema<B: Serialize>(
    context: &JsonPayloadContext<'_>,
    schema: &'static str,
    body: B,
) -> Result<Value, RunnerError> {
    serde_json::to_value(PayloadEnvelope {
        header: context.header(schema),
        body,
        footer: context.footer(),
    })
    .map_err(|error| RunnerError::Ui(format!("failed to encode tasks listing payload: {error}")))
}
