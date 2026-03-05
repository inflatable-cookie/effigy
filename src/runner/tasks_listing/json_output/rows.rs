use serde::Serialize;

use super::super::super::tasks_view::ManagedProfileDisplayRow;
use super::super::builtin_rows::prepare_builtin_task_rows;
use super::super::prepared_task_rows::PreparedCatalogTaskProjection;
use super::super::row_projection::{
    builtin_task_rows, BuiltinTaskRow, ProjectedCatalogTaskSignatureRow,
};

#[derive(Clone, Serialize)]
pub(super) struct TaskRowJson {
    task: Option<String>,
    run: Option<String>,
    manifest: String,
}

#[derive(Clone, Serialize)]
pub(super) struct BuiltinTaskRowJson {
    task: String,
    description: String,
}

#[derive(Clone, Serialize)]
pub(super) struct ManagedProfileRowJson {
    task: String,
    run: String,
    manifest: String,
    profile: String,
    invocation: String,
    parent_task: String,
}

impl TaskRowJson {
    pub(super) fn empty_owned(manifest: String) -> Self {
        Self {
            task: None,
            run: None,
            manifest,
        }
    }
}

pub(super) fn builtin_task_rows_json() -> Vec<BuiltinTaskRowJson> {
    builtin_rows_json(builtin_task_rows())
}

pub(super) fn builtin_rows_json<'a, I>(rows: I) -> Vec<BuiltinTaskRowJson>
where
    I: IntoIterator<Item = BuiltinTaskRow<'a>>,
{
    prepare_builtin_task_rows(rows)
        .into_iter()
        .map(|row| BuiltinTaskRowJson::new(row.task(), row.description()))
        .collect::<Vec<BuiltinTaskRowJson>>()
}

pub(super) fn catalog_and_managed_rows_json(
    prepared_rows: PreparedCatalogTaskProjection,
) -> (TaskRowJson, Vec<ManagedProfileRowJson>) {
    let (manifest, task_row, managed_rows) = prepared_rows.into_parts();
    let managed_rows = managed_rows
        .into_iter()
        .map(|row| ManagedProfileRowJson::from_display(row, manifest.as_str()))
        .collect::<Vec<ManagedProfileRowJson>>();
    let task_row = TaskRowJson::from_signature(task_row, manifest.as_str());
    (task_row, managed_rows)
}

impl TaskRowJson {
    fn from_signature(row: ProjectedCatalogTaskSignatureRow, manifest: &str) -> Self {
        Self {
            task: Some(row.task().to_owned()),
            run: Some(row.run().to_owned()),
            manifest: manifest.to_owned(),
        }
    }
}

impl BuiltinTaskRowJson {
    fn new(task: &str, description: &str) -> Self {
        Self {
            task: task.to_owned(),
            description: description.to_owned(),
        }
    }
}

impl ManagedProfileRowJson {
    fn from_display(row: ManagedProfileDisplayRow, manifest: &str) -> Self {
        Self {
            task: row.task,
            run: row.run,
            manifest: manifest.to_owned(),
            profile: row.profile,
            invocation: row.invocation,
            parent_task: row.parent_task,
        }
    }
}
