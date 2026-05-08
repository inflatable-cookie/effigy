use super::{
    data_list_report, data_transfer_report, effective_attach_mode, eject_generated_compose,
    load_all_container_policies, load_container_exec_working_dir, load_container_policy,
    load_inline_workspace_container_policy, load_workspace_ownership_targets,
    resolve_inline_workspace_exec_working_dir, stats_global_report, status_global_report, status_report,
    with_test_compose_backend, with_test_effigy_home, with_test_host_composer_home,
    AllocatedPortsSummary, ContainerDataTransferAction, ContainerDataVolumeEntry,
    ContainerPolicyError, ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry,
    ContainerStatusService, EffectiveAttachMode, EffectiveComposeSource, SharedServiceBinding,
};
use crate::compose::ComposeBackend;
use effigy_catalog::volumes::VolumeClassification;
use effigy_manifest::{load_task_manifest, ManifestInlineWorkspaceContainerConfig};
use std::fs;
use std::path::{Path, PathBuf};

mod compose;
mod policies;
mod volumes_reports;

pub(super) fn temp_repo(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "effigy-containers-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    fs::create_dir_all(&root).expect("mkdir");
    root
}

pub(super) fn with_temp_effigy_home<T>(name: &str, run: impl FnOnce(PathBuf) -> T) -> T {
    let home = temp_repo(&format!("home-{name}")).join(".effigy");
    fs::create_dir_all(&home).expect("mkdir effigy home");
    with_test_effigy_home(&home, || run(home.clone()))
}
