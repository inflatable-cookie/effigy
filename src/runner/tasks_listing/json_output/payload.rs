use serde::Serialize;
use serde_json::Value;

use super::super::super::RunnerError;
use super::rows::{BuiltinTaskRowJson, ManagedProfileRowJson, TaskRowJson};

const TASKS_SCHEMA: &str = "effigy.tasks.v1";
const FILTERED_TASKS_SCHEMA: &str = "effigy.tasks.filtered.v1";
const SCHEMA_VERSION: u64 = 1;

pub(super) struct JsonPayloadContext<'a> {
    catalog_count: usize,
    catalog_diagnostics: &'a [Value],
    precedence: &'a [String],
    resolve_probe: &'a Option<Value>,
}

pub(super) struct JsonTaskRows {
    task_rows: Vec<TaskRowJson>,
    managed_profile_rows: Vec<ManagedProfileRowJson>,
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
    catalog_tasks: Vec<TaskRowJson>,
    managed_profiles: Vec<ManagedProfileRowJson>,
    builtin_tasks: Vec<BuiltinTaskRowJson>,
}

#[derive(Serialize)]
struct FilteredPayloadBody {
    filter: String,
    matches: Vec<TaskRowJson>,
    managed_profile_matches: Vec<ManagedProfileRowJson>,
    builtin_matches: Vec<BuiltinTaskRowJson>,
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
}

impl JsonTaskRows {
    pub(super) fn new(
        task_rows: Vec<TaskRowJson>,
        managed_profile_rows: Vec<ManagedProfileRowJson>,
    ) -> Self {
        Self {
            task_rows,
            managed_profile_rows,
        }
    }

    fn into_parts(self) -> (Vec<TaskRowJson>, Vec<ManagedProfileRowJson>) {
        (self.task_rows, self.managed_profile_rows)
    }
}

fn encode_payload(payload: impl Serialize) -> Result<Value, RunnerError> {
    serde_json::to_value(payload).map_err(|error| {
        RunnerError::Ui(format!("failed to encode tasks listing payload: {error}"))
    })
}

pub(super) fn encode_catalog_payload(
    context: &JsonPayloadContext<'_>,
    rows: JsonTaskRows,
    builtin_tasks: Vec<BuiltinTaskRowJson>,
) -> Result<Value, RunnerError> {
    let (task_rows, managed_profile_rows) = rows.into_parts();
    encode_enveloped_payload(
        context,
        TASKS_SCHEMA,
        CatalogPayloadBody {
            catalog_tasks: task_rows,
            managed_profiles: managed_profile_rows,
            builtin_tasks,
        },
    )
}

pub(super) fn encode_filtered_payload(
    context: &JsonPayloadContext<'_>,
    filter: &str,
    rows: JsonTaskRows,
    builtin_matches: Vec<BuiltinTaskRowJson>,
    notes: Vec<String>,
) -> Result<Value, RunnerError> {
    let (task_rows, managed_profile_rows) = rows.into_parts();
    encode_enveloped_payload(
        context,
        FILTERED_TASKS_SCHEMA,
        FilteredPayloadBody {
            filter: filter.to_owned(),
            matches: task_rows,
            managed_profile_matches: managed_profile_rows,
            builtin_matches,
            notes,
        },
    )
}

impl JsonPayloadContext<'_> {
    fn envelope<B: Serialize>(&self, schema: &'static str, body: B) -> PayloadEnvelope<B> {
        PayloadEnvelope {
            header: PayloadHeader {
                schema,
                schema_version: SCHEMA_VERSION,
                catalog_count: self.catalog_count,
            },
            body,
            footer: SharedPayloadFooter {
                catalogs: self.catalog_diagnostics.to_vec(),
                precedence: self.precedence.to_vec(),
                resolve: self.resolve_probe.clone(),
            },
        }
    }
}

fn encode_enveloped_payload<B: Serialize>(
    context: &JsonPayloadContext<'_>,
    schema: &'static str,
    body: B,
) -> Result<Value, RunnerError> {
    encode_payload(context.envelope(schema, body))
}
