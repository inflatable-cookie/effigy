use super::super::execute::{catalog_task_label, task_run_preview};
use super::super::tasks_view::managed_profile_display_rows;
use super::super::{LoadedCatalog, ManifestTask};

pub(super) struct TaskRunProjection {
    pub(super) task: String,
    pub(super) run: String,
}

pub(super) struct ManagedProfileProjection {
    pub(super) task: String,
    pub(super) run: String,
    pub(super) profile: String,
    pub(super) invocation: String,
    pub(super) parent_task: String,
}

pub(super) fn project_task_run(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> TaskRunProjection {
    TaskRunProjection {
        task: catalog_task_label(catalog, task_name),
        run: task_run_preview(task),
    }
}

pub(super) fn project_managed_profiles(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<ManagedProfileProjection> {
    managed_profile_display_rows(catalog, task_name, task)
        .into_iter()
        .map(|row| ManagedProfileProjection {
            task: row.task,
            run: row.run,
            profile: row.profile,
            invocation: row.invocation,
            parent_task: row.parent_task,
        })
        .collect::<Vec<ManagedProfileProjection>>()
}
