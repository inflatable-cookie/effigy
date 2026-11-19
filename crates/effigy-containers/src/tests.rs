use super::{
    data_list_report, data_transfer_report, effective_attach_mode, load_all_container_policies,
    load_container_exec_working_dir, load_container_policy, load_container_policy_for_creation,
    load_inline_workspace_container_policy, load_workspace_ownership_targets,
    resolve_inline_workspace_exec_working_dir, stats_global_report, status_global_report,
    status_report, with_test_compose_backend, with_test_effigy_home, with_test_host_composer_home,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerPolicyError,
    ContainerServiceHealth, ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry,
    ContainerStatusService, EffectiveAttachMode, EffectiveComposeSource, SharedServiceBinding,
    WorkspaceExtraMountDecisionKind,
};
use crate::compose::ComposeBackend;
use effigy_catalog::volumes::VolumeClassification;
use effigy_manifest::ManifestInlineWorkspaceContainerConfig;
use std::fs;
use std::path::{Path, PathBuf};

mod compose;
mod data_from;
mod policies;
mod volumes_reports;

pub(super) fn temp_repo(name: &str) -> PathBuf {
    effigy_test_support::Fixture::process_lifetime(&format!("effigy-containers-{name}-"))
        .expect("fixture directory")
}

pub(super) fn with_temp_effigy_home<T>(name: &str, run: impl FnOnce(PathBuf) -> T) -> T {
    let home = temp_repo(&format!("home-{name}")).join(".effigy");
    fs::create_dir_all(&home).expect("mkdir effigy home");
    with_test_effigy_home(&home, || run(home.clone()))
}

/// Fixtures predate the v0.16 `compose_file` retirement: write the catalog
/// fragments and manifest rewrite `effigy container migrate` produces for every
/// environment that still declares the retired key, then resolve the ordinary
/// catalog declaration.
pub(super) fn migrate_declared_compose_files(repo_root: &Path) {
    let manifest_path = repo_root.join(effigy_manifest::TASK_MANIFEST_FILE);
    let Ok(manifest) = effigy_manifest::load_task_manifest(&manifest_path) else {
        return;
    };
    let Some(containers) = manifest.containers.as_ref() else {
        return;
    };
    let declared = containers
        .environments
        .iter()
        .filter(|(_, config)| config.compose_file.is_some())
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    for name in declared {
        let plan = crate::policy::compose_file::plan_migration(repo_root, Some(&name))
            .unwrap_or_else(|error| panic!("{}: {error}", repo_root.display()));
        for fragment in &plan.migration.fragments {
            fs::create_dir_all(&fragment.directory).expect("fragment directory");
            fs::write(
                fragment.directory.join("service.toml"),
                &fragment.service_toml,
            )
            .expect("service.toml");
            fs::write(
                fragment.directory.join("compose.fragment.yml"),
                &fragment.compose_fragment,
            )
            .expect("compose fragment");
        }
        fs::write(&plan.manifest, &plan.manifest_after).expect("migrated manifest");
    }
}

/// A fixture policy load: migrate any retired `compose_file` declaration first.
pub(super) fn fixture_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<crate::EffectiveContainerPolicy, ContainerPolicyError> {
    migrate_declared_compose_files(repo_root);
    load_container_policy(repo_root, requested_name)
}

/// A fixture creation policy: migrate any retired `compose_file` first.
pub(super) fn fixture_policy_for_creation(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<
    (
        crate::EffectiveContainerPolicy,
        Vec<crate::WorkspaceExtraMountDecision>,
    ),
    ContainerPolicyError,
> {
    migrate_declared_compose_files(repo_root);
    load_container_policy_for_creation(repo_root, requested_name)
}

/// Every declared environment's fixture policy: migrate retired keys first.
pub(super) fn fixture_all_policies(
    repo_root: &Path,
) -> Result<Vec<crate::EffectiveContainerPolicy>, ContainerPolicyError> {
    migrate_declared_compose_files(repo_root);
    load_all_container_policies(repo_root)
}

/// A fixture prepared policy: migrate any retired `compose_file` declaration first.
pub(super) fn fixture_prepared_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<crate::EffectiveContainerPolicy, ContainerPolicyError> {
    migrate_declared_compose_files(repo_root);
    crate::prepare_container_policy(repo_root, requested_name)
}
