use std::path::Path;

use super::super::execute::catalog_task_label;
use super::super::manifest::task_runtime::ManifestTask;
use effigy_managed::profiles::DEFAULT_MANAGED_PROFILE;
use effigy_manifest::LoadedCatalog;

#[derive(Debug)]
pub(in crate::runner) struct ManagedProfileDisplayRow {
    pub(in crate::runner) task: String,
    pub(in crate::runner) run: String,
    pub(in crate::runner) profile: String,
    pub(in crate::runner) invocation: String,
    pub(in crate::runner) parent_task: String,
}

pub(in crate::runner) fn relative_display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

pub(in crate::runner) fn managed_profile_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<ManagedProfileDisplayRow> {
    let Some(mode) = task.mode.as_deref() else {
        return Vec::new();
    };
    if task.profiles.is_empty() {
        return Vec::new();
    }
    let parent_task = catalog_task_label(catalog, task_name);
    task.profiles
        .keys()
        .filter(|profile| profile.as_str() != DEFAULT_MANAGED_PROFILE)
        .map(|profile| ManagedProfileDisplayRow {
            task: format!("{parent_task} {profile}"),
            run: format!("<managed:{mode} profile:{profile}>"),
            profile: profile.clone(),
            invocation: format!("{parent_task} {profile}"),
            parent_task: parent_task.clone(),
        })
        .collect()
}
