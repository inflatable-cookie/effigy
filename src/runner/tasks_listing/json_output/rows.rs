use serde::Serialize;

use super::super::super::{LoadedCatalog, ManifestTask};
use super::super::row_projection::{
    builtin_task_rows, project_builtin_rows, project_catalog_task_display_rows, BuiltinTaskRow,
    ProjectedCatalogTaskManifestRow, ProjectedManagedProfileManifestRow,
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
    project_builtin_rows(rows)
        .map(|row| BuiltinTaskRowJson::new(row.task(), row.description()))
        .collect::<Vec<BuiltinTaskRowJson>>()
}

pub(super) fn catalog_and_managed_rows_json<'a>(
    manifest: &'a str,
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> (
    TaskRowJson,
    impl Iterator<Item = ManagedProfileRowJson> + 'a,
) {
    let (task_row, managed_rows) =
        project_catalog_task_display_rows(catalog, task_name, task).into_manifest_rows(manifest);
    let managed_rows = managed_rows.map(ManagedProfileRowJson::from_projected);
    let task_row = TaskRowJson::from_projected(task_row);
    (task_row, managed_rows)
}

impl TaskRowJson {
    fn from_projected(row: ProjectedCatalogTaskManifestRow) -> Self {
        Self {
            task: Some(row.task),
            run: Some(row.run),
            manifest: row.manifest,
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
    fn from_projected(row: ProjectedManagedProfileManifestRow) -> Self {
        Self {
            task: row.task,
            run: row.run,
            manifest: row.manifest,
            profile: row.profile,
            invocation: row.invocation,
            parent_task: row.parent_task,
        }
    }
}
