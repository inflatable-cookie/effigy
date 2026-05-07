pub mod colima;
pub mod compose;
pub mod exec;
pub mod health;
mod mount_spec;
pub mod policy;
mod policy_support;
pub mod report;
mod runtime;
pub mod session;
mod workspace;

pub use policy::inline_workspace::{
    load_inline_workspace_container_policy, resolve_inline_workspace_exec_working_dir,
};
pub use policy::load::{
    effective_attach_mode, load_all_container_policies, load_container_exec_working_dir,
    load_container_policy, load_container_policy_with_workspace,
};
pub use policy::model::{
    ContainerEjectResult, ContainerPolicyError, EffectiveAttachMode, EffectiveComposeSource,
    EffectiveContainerPolicy, EffectiveDnsRoute, EffectiveHostProcess, EffectiveServiceAlias,
    HostProcessRestart, HostProcessSignal, SharedServiceBinding,
};
pub use report::{
    cache_list_all_report, cache_list_report, cache_prune_report, data_list_report,
    data_pull_production_report, data_transfer_report, down_report, eject_report, logs_report,
    reset_report, stats_all_report, status_all_report, status_report, up_detached_report,
    volume_list_report, AllocatedPortsSummary, ContainerCacheGlobalEntry, ContainerCachePruneEntry,
    ContainerCacheVolumeEntry, ContainerCommandReport, ContainerDataHookResult,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerStatsAllEntry,
    ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
    ContainerVolumeGlobalEntry,
};
pub use runtime::eject::eject_generated_compose;
pub use workspace::load_workspace_ownership_targets;

#[cfg(test)]
pub(crate) use compose::with_test_compose_backend;
pub use policy::validation::{validate_compose_backend_runtime, validate_container_policy};
#[cfg(test)]
pub(crate) use policy_support::with_test_effigy_home;
#[cfg(test)]
pub(crate) use workspace::with_test_host_composer_home;

use std::path::{Path, PathBuf};
use std::{fs, io};

use effigy_catalog::CatalogResolver;
use effigy_container_manager::BackendId;
use effigy_manifest::user_config::UserContainerBackendPreference;
use effigy_manifest::ManifestContainerDriver;

pub(crate) const DEFAULT_COLIMA_PROFILE: &str = "effigy";
pub(crate) const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
pub(crate) const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_RUNTIME_COMPOSE_DIR: &str = ".effigy/runtime/compose";
const RUNTIME_BACKEND_METADATA_FILE: &str = ".effigy/runtime/compose/.effigy-runtime.json";
pub(crate) const PROJECT_LOCAL_CATALOG_DIR: &str = "infra/dev/catalog";
const SHARED_SERVICE_HOST: &str = "host.docker.internal";
const NERDCTL_MOUNTS_LABEL_BUDGET_BYTES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CatalogNetworkContract {
    pub domain_label: String,
    pub container_port: u16,
    pub shared_service: bool,
    pub shared_host_env_vars: Vec<String>,
    pub shared_port_env_vars: Vec<String>,
}

pub(crate) fn resolve_catalog_network_contract(
    repo_root: Option<&Path>,
    catalog: &str,
) -> Result<Option<CatalogNetworkContract>, ContainerPolicyError> {
    let resolver = CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    );
    let fragment = resolver.resolve(catalog)?;
    let capabilities = fragment.schema.capabilities;
    let (Some(domain_label), Some(container_port)) = (
        capabilities.loopback_alias_label,
        capabilities.loopback_alias_port,
    ) else {
        return Ok(None);
    };
    Ok(Some(CatalogNetworkContract {
        domain_label,
        container_port,
        shared_service: capabilities.shared_service,
        shared_host_env_vars: capabilities.shared_host_env_vars,
        shared_port_env_vars: capabilities.shared_port_env_vars,
    }))
}

fn project_local_catalog_dir(repo_root: Option<&Path>) -> Option<PathBuf> {
    let repo_root = repo_root?;
    let path = repo_root.join(PROJECT_LOCAL_CATALOG_DIR);
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".effigy").join("catalog");
    path.is_dir().then_some(path)
}

pub fn user_global_backend_preference() -> Option<BackendId> {
    let user_config = effigy_manifest::load_user_config().ok()?;
    match user_config.preferred_container_backend()? {
        UserContainerBackendPreference::Containerd => Some(BackendId::colima_nerdctl()),
        UserContainerBackendPreference::Docker => Some(BackendId::docker_compose()),
    }
}

pub fn user_global_colima_profile() -> Option<String> {
    let user_config = effigy_manifest::load_user_config().ok()?;
    user_config.preferred_container_profile().map(str::to_owned)
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RuntimeBackendMetadata {
    backend: String,
}

pub fn load_runtime_backend_override(repo_root: &Path) -> Option<BackendId> {
    let path = repo_root.join(RUNTIME_BACKEND_METADATA_FILE);
    let source = fs::read_to_string(path).ok()?;
    let parsed = toml::from_str::<RuntimeBackendMetadata>(&source).ok()?;
    Some(BackendId::new(parsed.backend))
}

pub fn write_runtime_backend_override(
    repo_root: &Path,
    backend_id: &BackendId,
) -> Result<(), io::Error> {
    let path = repo_root.join(RUNTIME_BACKEND_METADATA_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let rendered = toml::to_string_pretty(&RuntimeBackendMetadata {
        backend: backend_id.as_str().to_owned(),
    })
    .map_err(io::Error::other)?;
    fs::write(path, rendered)
}

pub fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests;
