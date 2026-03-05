use serde::Serialize;

use super::super::super::{LoadedCatalog, ManifestTask, BUILTIN_TASKS};
use super::super::row_projection::{project_builtin_rows, project_catalog_task_display_rows};

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
    builtin_rows_json(BUILTIN_TASKS.iter().copied())
}

pub(super) fn builtin_rows_json<'a, I>(rows: I) -> Vec<BuiltinTaskRowJson>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    project_builtin_rows(rows)
        .map(|(task, description)| BuiltinTaskRowJson::new(task, description))
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
    let managed_rows = managed_rows.map(ManagedProfileRowJson::from_parts);
    let task_row = TaskRowJson::from_parts(task_row);
    (task_row, managed_rows)
}

impl TaskRowJson {
    fn from_parts((task, run, manifest): (String, String, String)) -> Self {
        Self {
            task: Some(task),
            run: Some(run),
            manifest,
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
    fn from_parts(
        (task, run, manifest, profile, invocation, parent_task): (
            String,
            String,
            String,
            String,
            String,
            String,
        ),
    ) -> Self {
        Self {
            task,
            run,
            manifest,
            profile,
            invocation,
            parent_task,
        }
    }
}
