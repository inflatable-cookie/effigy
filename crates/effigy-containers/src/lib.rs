pub mod backend;
mod catalog_sources;
pub mod colima;
pub mod compose;
pub mod environment;
pub mod exec;
pub mod health;
pub mod host_load;
pub mod manager;
mod mount_spec;
pub mod ops;
pub mod policy;
mod policy_support;
pub mod report;
mod runtime;
pub mod session;
mod workspace;

pub use effigy_core::host_alias::HostAlias;
pub use manager::*;
pub use ops::{
    ContainerCacheListOperation, ContainerCacheOperation, ContainerCachePruneOperation,
    ContainerCapturedExecOperation, ContainerConfirmationPolicy, ContainerDataOperation,
    ContainerDataTransferOperation, ContainerDownOperation, ContainerDumpOperation,
    ContainerExecOperation, ContainerLifecycleOperation, ContainerLogsOperation,
    ContainerOperationKind, ContainerOperationPlan, ContainerOperationRequest,
    ContainerOperationResult, ContainerPromptedOperation, ContainerReadOperation,
    ContainerResetOperation, ContainerShellOperation, ContainerSideEffectClass,
    ContainerStatsOperation, ContainerStatusOperation, ContainerUpOperation,
    ContainerVolumeListOperation, ContainerVolumeOperation, ContainerVolumePruneOperation,
};
pub use policy::data_from::{
    borrowed_volume_ensure_commands, plan_volume_data_handover, VolumeDataHandover,
};
pub use policy::host_alias::{DeclaredAlias, HostExecutionAliases};
pub use policy::host_process_env::{
    validate_host_process_env, HostProcessEnvFacts, HostProcessUrls, ServiceEndpoint,
};
pub use policy::hosts::{
    add_managed_listener_routes, build_host_map, build_host_map_including_listeners,
    domain_has_suffix, infer_base_domain, resolve_base_domain, route_scope_host_key,
    uses_shared_runtime_identity, EffectiveHostMap, EffectiveHostRoute, HostRouteKind,
    HostScopeKind, InferredBaseDomain,
};
pub use policy::inline_workspace::{
    load_inline_workspace_container_policy, resolve_inline_workspace_exec_working_dir,
};
pub use policy::load::{
    declares_container_registry, effective_attach_mode, load_all_container_policies,
    load_container_exec_target, load_container_exec_working_dir, load_container_policy,
    load_container_policy_for_creation, load_container_policy_for_creation_observed,
    load_container_policy_with_workspace, prepare_container_policy, ContainerExecTarget,
    CreationPolicyStage,
};
pub use policy::model::{
    ContainerPolicyError, EffectiveAttachMode, EffectiveComposeSource, EffectiveContainerPolicy,
    EffectiveDataPeer, EffectiveDnsRoute, EffectiveHostProcess, EffectiveManagedHostListener,
    EffectiveServiceAlias, HostProcessRestart, HostProcessSignal, SharedRender,
    SharedServiceBinding,
};
pub use policy::render::publish_render;
pub use report::{
    annotate_workspace_extra_mount_decisions, cache_list_global_report, cache_list_report,
    cache_prune_report, data_list_report, data_pull_production_report, data_transfer_report,
    down_noop_report, down_report, hosts_report, logs_report, reset_report, retire_report,
    stats_global_report, status_global_report, status_report, up_detached_report,
    volume_list_report, volume_prune_report, workspace_extra_mount_decision_lines,
    ContainerCacheGlobalEntry, ContainerCachePruneEntry, ContainerCacheVolumeEntry,
    ContainerCommandReport, ContainerDataHookResult, ContainerDataTransferAction,
    ContainerDataVolumeEntry, ContainerServiceHealth, ContainerStatsAllEntry,
    ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
    ContainerVolumeGlobalEntry, ContainerVolumePruneEntry,
};
pub use workspace::resolve_operator_workspace_for_container;
/// The Effigy state home for this process: `~/.effigy`, or the private
/// fixture home while unit tests run. Callers that touch `ports.json` or other
/// shared state must resolve it here rather than reading `HOME` directly.
pub fn effigy_home() -> Option<std::path::PathBuf> {
    policy_support::effigy_home_dir()
}

/// The gateway address a Colima profile's own configuration names, as an
/// observation the workspace channel can refuse on. It distinguishes a
/// configured address from an absent profile, an unreadable one and a profile
/// that configures none, instead of promoting the renderer's default.
pub use runtime::dns::observed_runtime_gateway_address as runtime_gateway_observation;
pub use runtime::retire::{
    unique_resources, FailedResource, GatewayRelease, KeptResource, OwnedResource, ResourceKind,
    RetireOutcome,
};
pub use runtime::scope::{
    named_volume_belongs_to, CHECKOUT_LABEL, COMPOSE_PROJECT_LABEL, MANAGED_LABEL,
    MOUNT_TARGET_LABEL, PERSIST_LABEL, PROJECT_LABEL, REPO_ROOT_LABEL, SCOPE_LABEL, SERVICE_LABEL,
    SHARED_RENDER_LABEL, VOLUME_NAME_LABEL,
};
pub use workspace::{
    load_workspace_ownership_plan, load_workspace_ownership_targets, rust_cache_kind,
    WorkspaceExtraMountDecision, WorkspaceExtraMountDecisionKind, WorkspaceMountKind,
    WorkspaceOwnershipPlan, WorkspaceOwnershipTarget, WorkspaceRepairAuthority,
    WorkspaceRustCacheKind,
};

#[cfg(test)]
pub(crate) use compose::with_test_compose_backend;
pub use policy::validation::{validate_compose_backend_runtime, validate_container_policy};
#[cfg(any(test, feature = "test-support"))]
pub use policy_support::with_test_effigy_home;
#[cfg(test)]
pub(crate) use workspace::with_test_host_composer_home;

#[cfg(test)]
pub(crate) static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
pub(crate) fn test_env_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

use std::path::Path;

use effigy_manifest::user_config::UserContainerBackendPreference;
use effigy_manifest::ManifestContainerDriver;

pub(crate) const DEFAULT_COLIMA_PROFILE: &str = "effigy";
pub(crate) const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
pub(crate) const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_RUNTIME_COMPOSE_DIR: &str = ".effigy/runtime/compose";
pub(crate) use catalog_sources::PROJECT_LOCAL_CATALOG_DIR;
pub use catalog_sources::{catalog_resolver, catalog_service_sources, CatalogServiceSource};
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
    let fragment = catalog_resolver(repo_root)?.resolve(catalog)?;
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

pub fn user_global_colima_profile_disk_gib() -> Option<u64> {
    let user_config = effigy_manifest::load_user_config().ok()?;
    user_config.preferred_container_profile_disk_gib()
}

pub fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests;
