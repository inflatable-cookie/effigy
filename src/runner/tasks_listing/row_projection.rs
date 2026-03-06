use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::manifest::task_runtime::ManifestTask;
use super::super::model::catalog::LoadedCatalog;
use super::super::model::constants::BUILTIN_TASKS;
use super::super::tasks_view::{managed_profile_display_rows, ManagedProfileDisplayRow};

pub(super) struct TaskSignatureProjection {
    task: String,
    run: String,
}

pub(super) type BuiltinTaskProjection<'a> = (&'a str, &'a str);

impl TaskSignatureProjection {
    fn new(task: String, run: String) -> Self {
        Self { task, run }
    }

    pub(super) fn task(&self) -> &str {
        self.task.as_str()
    }

    pub(super) fn run(&self) -> &str {
        self.run.as_str()
    }
}

pub(super) fn builtin_task_rows() -> impl Iterator<Item = BuiltinTaskProjection<'static>> {
    BUILTIN_TASKS.iter().copied()
}

pub(super) fn project_catalog_task_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> (TaskSignatureProjection, Vec<ManagedProfileDisplayRow>) {
    let task_row = TaskSignatureProjection::new(
        catalog_task_label(catalog, task_name),
        task_run_preview(task),
    );
    let managed_profiles = managed_profile_display_rows(catalog, task_name, task);
    (task_row, managed_profiles)
}
