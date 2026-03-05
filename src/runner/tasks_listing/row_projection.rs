use std::iter;

use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::{managed_profile_display_rows, ManagedProfileDisplayRow};
use super::super::{LoadedCatalog, ManifestTask, BUILTIN_TASKS};

pub(super) struct ProjectedCatalogTaskSignatureRow {
    task: String,
    run: String,
}

pub(super) struct ProjectedCatalogTaskManifestRow {
    pub(super) task: String,
    pub(super) run: String,
    pub(super) manifest: String,
}

pub(super) struct ProjectedManagedProfileManifestRow {
    pub(super) task: String,
    pub(super) run: String,
    pub(super) manifest: String,
    pub(super) profile: String,
    pub(super) invocation: String,
    pub(super) parent_task: String,
}

#[derive(Clone, Copy)]
pub(super) struct BuiltinTaskRow<'a> {
    task: &'a str,
    description: &'a str,
}

pub(super) struct ProjectedCatalogTaskRows {
    task: ProjectedCatalogTaskSignatureRow,
    managed_profiles: std::vec::IntoIter<ManagedProfileDisplayRow>,
}

impl ProjectedCatalogTaskRows {
    pub(super) fn into_signature_rows(
        self,
    ) -> impl Iterator<Item = ProjectedCatalogTaskSignatureRow> {
        iter::once(self.task).chain(
            self.managed_profiles
                .map(ProjectedCatalogTaskSignatureRow::from_managed),
        )
    }

    pub(super) fn into_manifest_rows<'a>(
        self,
        manifest: &'a str,
    ) -> (
        ProjectedCatalogTaskManifestRow,
        impl Iterator<Item = ProjectedManagedProfileManifestRow> + 'a,
    ) {
        let task_row = ProjectedCatalogTaskManifestRow::from_signature(self.task, manifest);
        (
            task_row,
            self.managed_profiles
                .map(move |row| ProjectedManagedProfileManifestRow::from_managed(row, manifest)),
        )
    }
}

impl ProjectedCatalogTaskSignatureRow {
    fn new(task: String, run: String) -> Self {
        Self { task, run }
    }

    fn from_managed(row: ManagedProfileDisplayRow) -> Self {
        Self::new(row.task, row.run)
    }

    pub(super) fn task(&self) -> &str {
        self.task.as_str()
    }

    pub(super) fn run(&self) -> &str {
        self.run.as_str()
    }
}

impl ProjectedCatalogTaskManifestRow {
    fn from_signature(row: ProjectedCatalogTaskSignatureRow, manifest: &str) -> Self {
        Self {
            task: row.task,
            run: row.run,
            manifest: manifest.to_owned(),
        }
    }
}

impl ProjectedManagedProfileManifestRow {
    fn from_managed(row: ManagedProfileDisplayRow, manifest: &str) -> Self {
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

impl<'a> BuiltinTaskRow<'a> {
    pub(super) fn new(task: &'a str, description: &'a str) -> Self {
        Self { task, description }
    }

    pub(super) fn task(&self) -> &'a str {
        self.task
    }

    pub(super) fn description(&self) -> &'a str {
        self.description
    }
}

pub(super) fn project_builtin_rows<'a, I>(rows: I) -> impl Iterator<Item = BuiltinTaskRow<'a>>
where
    I: IntoIterator<Item = BuiltinTaskRow<'a>>,
{
    rows.into_iter()
}

pub(super) fn builtin_task_rows() -> impl Iterator<Item = BuiltinTaskRow<'static>> {
    BUILTIN_TASKS
        .iter()
        .map(|(task, description)| BuiltinTaskRow::new(task, description))
}

pub(super) fn project_catalog_task_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> ProjectedCatalogTaskRows {
    let task_row = ProjectedCatalogTaskSignatureRow::new(
        catalog_task_label(catalog, task_name),
        task_run_preview(task),
    );

    ProjectedCatalogTaskRows {
        task: task_row,
        managed_profiles: managed_profile_display_rows(catalog, task_name, task).into_iter(),
    }
}
