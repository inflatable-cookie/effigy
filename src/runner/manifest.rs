use std::path::Path;

use effigy_manifest::ManifestError;
use effigy_tasks::{render_task_selector, TaskSelector};

use super::locking::model::LockScope;

pub(in crate::runner) use effigy_manifest::{config_sections, task_runtime};
pub(in crate::runner) use effigy_manifest::{
    LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode, ManifestDocsPolicyConfig,
    ManifestManagedRun, ManifestTask, TaskManifest,
};

pub(super) fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(map_manifest_error)
}

pub(in crate::runner) fn load_task_manifest_with_inspection(
    manifest_path: &Path,
) -> Result<LoadedTaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest_with_inspection(manifest_path).map_err(map_manifest_error)
}

pub(super) fn selector_lock_name(selector: &TaskSelector) -> String {
    render_task_selector(selector)
}

pub(super) fn task_lock_scope(task: &ManifestTask, selector: &TaskSelector) -> LockScope {
    match task.lock.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => LockScope::Shared(name.to_owned()),
        _ => LockScope::Task(selector_lock_name(selector)),
    }
}

pub(super) fn map_manifest_error(error: ManifestError) -> super::RunnerError {
    match error {
        ManifestError::Read { path, error } => super::RunnerError::TaskManifestRead { path, error },
        ManifestError::Parse { path, error } => {
            super::RunnerError::TaskManifestParse { path, error }
        }
        ManifestError::Compose { path, detail } => {
            super::RunnerError::TaskManifestCompose { path, detail }
        }
        ManifestError::Render { path, detail } => {
            super::RunnerError::task_invocation_failed_render(&path, detail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{selector_lock_name, task_lock_scope};
    use crate::runner::locking::model::LockScope;
    use effigy_manifest::ManifestTask;
    use effigy_tasks::TaskSelector;

    #[test]
    fn default_task_lock_scope_uses_rendered_selector_for_prefixed_tasks() {
        let selector = TaskSelector {
            prefix: Some("acme-api".to_owned()),
            task_name: "dev".to_owned(),
        };
        assert_eq!(selector_lock_name(&selector), "acme-api/dev");
        assert_eq!(
            task_lock_scope(&ManifestTask::default(), &selector),
            LockScope::Task("acme-api/dev".to_owned())
        );
    }
}
