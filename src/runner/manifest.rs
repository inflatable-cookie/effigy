use std::path::Path;

use effigy_core::build_info::stale_repo_local_install;
use effigy_manifest::ManifestError;

pub(in crate::runner) use effigy_manifest::config_sections;
pub(in crate::runner) use effigy_manifest::{
    LoadedTaskManifest, ManifestDemoConfig, ManifestDemoMode, ManifestDocsPolicyConfig,
    ManifestManagedRun, ManifestTask, TaskManifest, TASK_MANIFEST_FILE,
};

pub(super) fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest(manifest_path).map_err(map_manifest_error)
}

pub(in crate::runner) fn load_task_manifest_with_inspection(
    manifest_path: &Path,
) -> Result<LoadedTaskManifest, super::RunnerError> {
    effigy_manifest::load_task_manifest_with_inspection(manifest_path).map_err(map_manifest_error)
}

pub(super) fn map_manifest_error(error: ManifestError) -> super::RunnerError {
    match error {
        ManifestError::Read { path, error } => super::RunnerError::TaskManifestRead { path, error },
        ManifestError::Parse { path, error } => {
            let stale_local_install = stale_repo_local_install(&path).map(Box::new);
            super::RunnerError::TaskManifestParse {
                path,
                error,
                stale_local_install,
            }
        }
        ManifestError::Compose { path, detail } => {
            super::RunnerError::TaskManifestCompose { path, detail }
        }
        ManifestError::Render { path, detail } => {
            super::RunnerError::task_invocation_failed_render(&path, detail)
        }
    }
}
