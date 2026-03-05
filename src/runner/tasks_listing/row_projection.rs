use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::{managed_profile_display_rows, ManagedProfileDisplayRow};
use super::super::{LoadedCatalog, ManifestTask, BUILTIN_TASKS};

pub(super) struct ProjectedCatalogTaskSignatureRow {
    task: String,
    run: String,
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
    pub(super) fn into_task_and_managed_rows(
        self,
    ) -> (
        ProjectedCatalogTaskSignatureRow,
        impl Iterator<Item = ManagedProfileDisplayRow>,
    ) {
        (self.task, self.managed_profiles)
    }
}

impl ProjectedCatalogTaskSignatureRow {
    fn new(task: String, run: String) -> Self {
        Self { task, run }
    }

    pub(super) fn from_managed_display(row: ManagedProfileDisplayRow) -> Self {
        Self::new(row.task, row.run)
    }

    pub(super) fn task(&self) -> &str {
        self.task.as_str()
    }

    pub(super) fn run(&self) -> &str {
        self.run.as_str()
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
