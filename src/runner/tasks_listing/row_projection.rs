use std::iter;

use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::{managed_profile_display_rows, ManagedProfileDisplayRow};
use super::super::{LoadedCatalog, ManifestTask};

pub(super) struct ProjectedCatalogTaskRows {
    task: (String, String),
    managed_profiles: std::vec::IntoIter<ManagedProfileDisplayRow>,
}

impl ProjectedCatalogTaskRows {
    pub(super) fn into_signature_rows(self) -> impl Iterator<Item = (String, String)> {
        iter::once(self.task).chain(self.managed_profiles.map(|row| (row.task, row.run)))
    }

    pub(super) fn into_manifest_rows<'a>(
        self,
        manifest: &'a str,
    ) -> (
        (String, String, String),
        impl Iterator<Item = (String, String, String, String, String, String)> + 'a,
    ) {
        let (task, run) = self.task;
        (
            (task, run, manifest.to_owned()),
            self.managed_profiles.map(move |row| {
                (
                    row.task,
                    row.run,
                    manifest.to_owned(),
                    row.profile,
                    row.invocation,
                    row.parent_task,
                )
            }),
        )
    }
}

pub(super) fn project_builtin_rows<'a, I>(rows: I) -> impl Iterator<Item = (&'a str, &'a str)>
where
    I: IntoIterator<Item = (&'a str, &'a str)>,
{
    rows.into_iter()
}

pub(super) fn project_catalog_task_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> ProjectedCatalogTaskRows {
    let task_row = (
        catalog_task_label(catalog, task_name),
        task_run_preview(task),
    );

    ProjectedCatalogTaskRows {
        task: task_row,
        managed_profiles: managed_profile_display_rows(catalog, task_name, task).into_iter(),
    }
}
