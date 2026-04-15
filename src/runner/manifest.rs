use std::path::Path;

use effigy_manifest::ManifestError;

use super::locking::model::LockScope;

pub(in crate::runner) use effigy_manifest::{config_sections, task_runtime};
pub(in crate::runner) use effigy_manifest::{
    LoadedTaskManifest, ManifestBootstrapSubmodulesPolicy, ManifestCargoEnvMatchMode,
    ManifestCompositionEdge, ManifestCompositionOverride, ManifestCompositionValueSource,
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestDemoConfig, ManifestDemoMode, ManifestDemoStatus, ManifestDocsPolicyConfig,
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRun, ManifestManagedRunStep,
    ManifestTask, ManifestTestSuiteTeardownPolicy, TaskManifest,
};

pub(super) fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(map_manifest_error)
}

pub(in crate::runner) fn load_task_manifest_with_inspection(
    manifest_path: &Path,
) -> Result<LoadedTaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest_with_inspection(manifest_path).map_err(map_manifest_error)
}

pub(super) fn task_lock_scope(task: &ManifestTask, task_name: &str) -> LockScope {
    match task.lock.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => LockScope::Shared(name.to_owned()),
        _ => LockScope::Task(task_name.to_owned()),
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
