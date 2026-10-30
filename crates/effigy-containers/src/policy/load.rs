use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_task_manifest_with_inspection, resolve_task_execution_binding_from_parts,
    ManifestContainerConfig, ManifestContainerDriver, ManifestContainerOnTaskExit,
    ManifestContainerSecretDelivery, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestTask, ManifestWorkspaceConfig, ResolvedTaskExecutionBinding,
    ResolvedWorkspaceContainer, TASK_MANIFEST_FILE,
};

use crate::mount_spec::resolve_host_mounts;
use crate::policy::hosts::{apply_scope_to_routes, host_key, rewrite_declared_host, scope_token};
use crate::policy::project::{
    default_project_name_base, resolve_project_name, validate_unique_project_names,
};
use crate::policy_support::resolve_compose_source;
use crate::runtime::dns::{materialize_runtime_dns_override, runtime_route_domains};
use crate::workspace::{
    materialize_runtime_workspace_mount_rewrite, resolve_operator_workspace_for_container,
    WorkspaceComposeRewrite, WorkspaceExtraMountDecision,
};
use crate::{
    resolve_catalog_network_contract, DEFAULT_ATTACH_TIMEOUT_SECS, DEFAULT_COLIMA_PROFILE,
    DEFAULT_HEALTH_TIMEOUT_SECS,
};

use super::model::{
    ComposeVolumeBorrow, ContainerPolicyError, EffectiveAttachMode, EffectiveComposeSource,
    EffectiveContainerPolicy, EffectiveDataPeer, EffectiveDnsRoute, EffectiveHostProcess,
    EffectiveManagedHostListener, EffectiveServiceAlias, HostProcessRestart, HostProcessSignal,
};

pub fn load_container_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    load_container_policy_with_workspace(repo_root, requested_name, None)
}

/// Resolve user-global library mounts for the manifest's declared bundle.
///
/// Returns an empty vec when the manifest has no `[bundle].base`, when the
/// user has no `~/.effigy/config.toml`, or when the file has no entry for
/// the active bundle. A malformed `config.toml` surfaces as a parse error.
fn resolve_library_mounts(
    manifest: &effigy_manifest::TaskManifest,
    bundle_root: Option<&Path>,
) -> Result<Vec<effigy_manifest::LibraryMount>, ContainerPolicyError> {
    let Some(_bundle_base) = manifest
        .bundle
        .as_ref()
        .and_then(|bundle| bundle.base.as_ref())
    else {
        return Ok(Vec::new());
    };
    let Some(bundle_root) = bundle_root else {
        return Ok(Vec::new());
    };
    let Some(bundle_name) = read_bundle_name_from_descriptor(bundle_root)? else {
        return Ok(Vec::new());
    };
    let user_config = effigy_manifest::load_user_config()?;
    Ok(user_config.library_mounts_for(&bundle_name))
}

fn read_bundle_name_from_descriptor(
    bundle_root: &Path,
) -> Result<Option<String>, ContainerPolicyError> {
    #[derive(serde::Deserialize)]
    struct BundleDescriptor {
        bundle: BundleMetadata,
    }

    #[derive(serde::Deserialize)]
    struct BundleMetadata {
        name: String,
    }

    let descriptor_path = bundle_root.join("bundle.toml");
    if !descriptor_path.is_file() {
        return Ok(None);
    }
    let descriptor_source = std::fs::read_to_string(&descriptor_path).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to read bundle descriptor at {}: {error}",
            descriptor_path.display()
        ))
    })?;
    let descriptor: BundleDescriptor = toml::from_str(&descriptor_source).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to parse bundle descriptor at {}: {error}",
            descriptor_path.display()
        ))
    })?;
    let bundle_name = descriptor.bundle.name.trim().to_owned();
    if bundle_name.is_empty() {
        return Ok(None);
    }
    Ok(Some(bundle_name))
}

pub fn load_container_policy_with_workspace(
    repo_root: &Path,
    requested_name: Option<&str>,
    workspace_override: Option<&ManifestWorkspaceConfig>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    load_container_policy_with_workspace_decisions(
        repo_root,
        requested_name,
        workspace_override,
        false,
    )
    .map(|(policy, _)| policy)
}

/// One preparation stage of a creation-time policy load, in the order the load
/// performs it. A caller that shows progress names the stage now running, so a
/// slow mount preparation or a blocked checkout read says what it is doing
/// instead of holding one fixed label (effigy#460, Q-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreationPolicyStage {
    /// Recording this checkout's identity before any scope-dependent policy.
    CheckoutIdentity,
    /// Resolving the declaration, its services, published ports and loopback
    /// allocation.
    Declaration,
    /// Preparing render inputs and the owned mount directories they name.
    RenderInputs,
    /// Re-resolving the declaration with the workspace and extra-mount
    /// decisions the render inputs produced. This is a second manifest and
    /// declaration read of its own, not the render-input preparation.
    ResolvedDeclaration,
    /// Rendering the Compose stack for the resolved generation.
    Render,
    /// Preparing the mount sources the stack binds.
    MountSources,
}

impl CreationPolicyStage {
    /// The stage in the operator's words.
    pub fn description(self) -> &'static str {
        match self {
            Self::CheckoutIdentity => "recording the checkout identity",
            Self::Declaration => "resolving the container declaration, ports and loopback",
            Self::RenderInputs => "preparing render inputs and owned mount directories",
            Self::ResolvedDeclaration => {
                "resolving the declaration again with the workspace and extra mounts"
            }
            Self::Render => "rendering the Compose stack",
            Self::MountSources => "preparing the mount sources the stack binds",
        }
    }
}

/// Resolve a fresh policy and optional extra-mount decisions for container up.
/// Prepare the checkout identity before resolving any scope-dependent policy.
pub fn load_container_policy_for_creation(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<(EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    load_container_policy_for_creation_observed(repo_root, requested_name, &mut |_| {})
}

/// The same load, reporting each preparation stage as it starts.
pub fn load_container_policy_for_creation_observed(
    repo_root: &Path,
    requested_name: Option<&str>,
    stage: &mut dyn FnMut(CreationPolicyStage),
) -> Result<(EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    stage(CreationPolicyStage::CheckoutIdentity);
    effigy_core::worktree_scope::prepare_checkout_token(repo_root).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "cannot record the checkout identity: {error}"
        ))
    })?;
    stage(CreationPolicyStage::Declaration);
    let declared = load_container_policy(repo_root, requested_name)?;
    stage(CreationPolicyStage::RenderInputs);
    super::render::prepare_inputs(&declared)?;
    stage(CreationPolicyStage::ResolvedDeclaration);
    let resolved =
        load_container_policy_with_workspace_decisions(repo_root, requested_name, None, true)?;
    stage(CreationPolicyStage::Render);
    super::render::prepare(&resolved.0)?;
    stage(CreationPolicyStage::MountSources);
    super::mounts::prepare_container_mount_sources(&resolved.0)?;
    Ok(resolved)
}

pub fn prepare_container_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let policy = load_container_policy(repo_root, requested_name)?;
    super::render::prepare_resolved(&policy)
}

fn load_container_policy_with_workspace_decisions(
    repo_root: &Path,
    requested_name: Option<&str>,
    workspace_override: Option<&ManifestWorkspaceConfig>,
    record_optional_extra_mount_decisions: bool,
) -> Result<(EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    load_policy(
        repo_root,
        requested_name,
        workspace_override,
        record_optional_extra_mount_decisions,
    )
}

fn load_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
    workspace_override: Option<&ManifestWorkspaceConfig>,
    record_optional_extra_mount_decisions: bool,
) -> Result<(EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.into(),
            error,
        })?;
    let repo_root = canonical_root.as_path();
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let default_project_name_base = default_project_name_base(&loaded, repo_root);
    validate_unique_project_names(containers, &default_project_name_base, repo_root)?;
    let name = resolve_container_name(&loaded.manifest, containers, requested_name)?;
    let inferred_workspace =
        resolve_container_workspace(&loaded.manifest, Some(&name), workspace_override)?;
    let config = containers.environments.get(&name).ok_or_else(|| {
        let available = containers
            .environments
            .keys()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", ");
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` is not defined in `[containers]` (available: {available})"
        ))
    })?;
    let library_mounts = resolve_library_mounts(&loaded.manifest, loaded.bundle_root.as_deref())?;
    build_effective_policy(PolicyBuildContext {
        repo_root,
        bundle_root: loaded.bundle_root.as_deref(),
        containers,
        default_project_name_base: &default_project_name_base,
        name: &name,
        config,
        effective_manifest: &loaded.effective_manifest,
        declarations: &loaded.evaluation_order,
        workspace: inferred_workspace.as_ref(),
        library_mounts: &library_mounts,
        record_optional_extra_mount_decisions,
    })
}

pub fn load_all_container_policies(
    repo_root: &Path,
) -> Result<Vec<EffectiveContainerPolicy>, ContainerPolicyError> {
    let canonical_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.into(),
            error,
        })?;
    let repo_root = canonical_root.as_path();
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let default_project_name_base = default_project_name_base(&loaded, repo_root);
    validate_unique_project_names(containers, &default_project_name_base, repo_root)?;
    let library_mounts = resolve_library_mounts(&loaded.manifest, loaded.bundle_root.as_deref())?;

    let mut policies = containers
        .environments
        .iter()
        .map(|(name, config)| {
            let workspace = resolve_container_workspace(&loaded.manifest, Some(name), None)?;
            build_effective_policy(PolicyBuildContext {
                repo_root,
                bundle_root: loaded.bundle_root.as_deref(),
                containers,
                default_project_name_base: &default_project_name_base,
                name,
                config,
                effective_manifest: &loaded.effective_manifest,
                declarations: &loaded.evaluation_order,
                workspace: workspace.as_ref(),
                library_mounts: &library_mounts,
                record_optional_extra_mount_decisions: false,
            })
            .map(|(policy, _)| policy)
        })
        .collect::<Result<Vec<_>, _>>()?;
    policies.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(policies)
}

fn resolve_container_workspace(
    manifest: &effigy_manifest::TaskManifest,
    requested_container_name: Option<&str>,
    workspace_override: Option<&ManifestWorkspaceConfig>,
) -> Result<Option<ManifestWorkspaceConfig>, ContainerPolicyError> {
    // Even an explicit override must not bypass conflicting declarations:
    // Prepare and later entry only retain the selected container identity.
    let declared = resolve_operator_workspace_for_container(manifest, requested_container_name)?;
    let mut workspace = workspace_override.cloned().or(declared);
    if let Some(workspace) = workspace.as_mut() {
        if let Some(catalog) = manifest.catalog.as_ref() {
            workspace
                .resolve_member_mounts(&catalog.members)
                .map_err(ContainerPolicyError::TaskInvocation)?;
        } else if workspace
            .mounts
            .iter()
            .any(|mount| mount.member().is_some())
        {
            workspace
                .resolve_member_mounts(&Default::default())
                .map_err(ContainerPolicyError::TaskInvocation)?;
        }
    }
    Ok(workspace)
}

/// Manifest-only execution coordinates, safe to resolve inside the guest.
/// No host catalog, Compose render, runtime registry or host mount is resolved.
#[derive(Debug)]
pub struct ContainerExecTarget {
    pub container: String,
    pub service: String,
    pub working_dir: PathBuf,
}

pub fn load_container_exec_target(
    repo_root: &Path,
    requested_name: Option<&str>,
    requested_service: Option<&str>,
) -> Result<ContainerExecTarget, ContainerPolicyError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let name = resolve_container_name(&loaded.manifest, containers, requested_name)?;
    let inferred_workspace =
        resolve_operator_workspace_for_container(&loaded.manifest, Some(&name))?;
    let config = containers.environments.get(&name).ok_or_else(|| {
        let available = containers
            .environments
            .keys()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", ");
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` is not defined in `[containers]` (available: {available})"
        ))
    })?;

    let primary_service = config.primary_service.clone().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` must declare `primary_service`"
        ))
    })?;
    let service = requested_service
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&primary_service)
        .to_owned();
    let working_dir = if service == primary_service {
        resolve_container_exec_working_dir(repo_root, &name, config, inferred_workspace.as_ref())?
    } else {
        repo_root.to_path_buf()
    };
    Ok(ContainerExecTarget {
        container: name,
        service,
        working_dir,
    })
}

pub fn load_container_exec_working_dir(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<PathBuf, ContainerPolicyError> {
    Ok(load_container_exec_target(repo_root, requested_name, None)?.working_dir)
}

pub fn effective_attach_mode(
    policy: &EffectiveContainerPolicy,
    attach: bool,
    detach: bool,
) -> EffectiveAttachMode {
    if attach {
        return EffectiveAttachMode::Attached;
    }
    if detach {
        return EffectiveAttachMode::Detached;
    }
    match policy.startup {
        ManifestContainerStartup::Attached => EffectiveAttachMode::Attached,
        ManifestContainerStartup::Detached => EffectiveAttachMode::Detached,
    }
}

struct PolicyBuildContext<'a> {
    repo_root: &'a Path,
    bundle_root: Option<&'a Path>,
    containers: &'a ManifestContainersConfig,
    default_project_name_base: &'a str,
    name: &'a str,
    config: &'a ManifestContainerConfig,
    effective_manifest: &'a str,
    declarations: &'a [PathBuf],
    workspace: Option<&'a ManifestWorkspaceConfig>,
    library_mounts: &'a [effigy_manifest::LibraryMount],
    record_optional_extra_mount_decisions: bool,
}

fn build_effective_policy(
    context: PolicyBuildContext<'_>,
) -> Result<(EffectiveContainerPolicy, Vec<WorkspaceExtraMountDecision>), ContainerPolicyError> {
    let PolicyBuildContext {
        repo_root,
        bundle_root,
        containers,
        default_project_name_base,
        name,
        config,
        effective_manifest,
        declarations,
        workspace,
        library_mounts,
        record_optional_extra_mount_decisions,
    } = context;
    // Retired `compose_file` resolves as the catalog declaration that
    // `effigy container migrate` would write, so both forms render alike.
    let translated = super::compose_file::translate_for_policy(repo_root, name, config)?;
    let config = translated.as_ref().unwrap_or(config);
    let mut draft = super::render::Draft::default();
    draft.declarations(declarations)?;
    let driver = config.driver.unwrap_or(ManifestContainerDriver::Colima);
    let profile = config
        .profile
        .clone()
        .unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned());
    let project_name = resolve_project_name(
        config,
        default_project_name_base,
        name,
        containers.environments.len(),
        repo_root,
    )?;
    let data_links = resolve_volume_data_links(
        repo_root,
        containers,
        default_project_name_base,
        name,
        config,
    )?;
    let volume_borrow = data_links.borrow.clone();
    let (
        mut compose_files,
        compose_file_display,
        managed_volumes,
        borrowed_volumes,
        effective_ports,
        shared_services,
    ) = resolve_compose_source(
        repo_root,
        bundle_root,
        name,
        config,
        &project_name,
        effective_manifest,
        volume_borrow.as_ref(),
        &mut draft,
    )?;
    let primary_service = config.primary_service.clone().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` must declare `primary_service`"
        ))
    })?;
    let mut workspace_mount_decisions = Vec::new();
    if driver == ManifestContainerDriver::Colima {
        if let Some(workspace) = workspace {
            let working_dir =
                resolve_container_exec_working_dir(repo_root, name, config, Some(workspace))?;
            workspace_mount_decisions = materialize_runtime_workspace_mount_rewrite(
                WorkspaceComposeRewrite {
                    repo_root,
                    container_name: name,
                    config,
                    workspace,
                    working_dir: &working_dir,
                    primary_service: &primary_service,
                    library_mounts,
                    managed_volumes: &managed_volumes,
                },
                &mut compose_files,
                record_optional_extra_mount_decisions,
                &mut draft,
            )?;
        }
    }
    let dns = config.dns.as_ref().cloned().unwrap_or_default();
    let dns_base_domain = dns.base_domain.clone();
    let mut dns_routes = Vec::new();
    for route in dns.resolved_routes() {
        if route.domain.trim().is_empty() {
            continue;
        }
        dns_routes.push(EffectiveDnsRoute {
            domain: route.domain.clone(),
            declared_domain: route.domain,
            tls: route.tls.unwrap_or(false),
            port: route.port,
            service: route.service.filter(|value| !value.trim().is_empty()),
            target_host: route
                .target_host
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        });
    }
    let runtime_token = scope_token(repo_root, config.share_runtime_identity)?;
    let runtime_host_key = runtime_token.as_deref().and_then(host_key);
    if let Some(key) = runtime_host_key {
        apply_scope_to_routes(&mut dns_routes, key);
    }
    let service_aliases = effective_service_aliases(repo_root, config)?;
    // One apex for generated TCP aliases, Compose extra_hosts and gateway
    // registration: the declared `base_domain`, else the shallowest host the
    // primary service or a managed listener serves.
    let listener_declared_domains = config
        .host_processes
        .iter()
        .filter_map(|entry| entry.listener.as_ref())
        .map(|listener| listener.route.domain.clone())
        .collect::<Vec<_>>();
    let resolved_declared_base = super::hosts::resolve_base_domain(
        &dns_routes,
        &listener_declared_domains,
        dns_base_domain.as_deref(),
    );
    let resolved_effective_base = resolved_declared_base.as_deref().map(|base_domain| {
        runtime_host_key.map_or_else(
            || base_domain.to_owned(),
            |key| rewrite_declared_host(base_domain, key),
        )
    });
    if driver == ManifestContainerDriver::Colima {
        let runtime_routes = runtime_route_domains(
            &dns_routes,
            &service_aliases,
            resolved_declared_base.as_deref(),
            resolved_effective_base.as_deref(),
        );
        materialize_runtime_dns_override(
            repo_root,
            name,
            &profile,
            &runtime_routes,
            &mut compose_files,
            &mut draft,
        )?;
    }
    let host = config.host.as_ref().cloned().unwrap_or_default();
    let resolved_host_mounts = resolve_host_mounts(repo_root, name, &host.mounts)?;
    let data = config.data.as_ref().cloned().unwrap_or_default();
    let health = config.health.as_ref().cloned().unwrap_or_default();
    let lifecycle = config.lifecycle.as_ref();
    let secrets = config.secrets.as_ref().cloned().unwrap_or_default();
    let default_workspace_identity =
        default_workspace_identity_for_primary_service(config, &primary_service);
    let _ = containers;

    let mut policy = EffectiveContainerPolicy {
        render: None,
        repo_root: repo_root.to_path_buf(),
        name: name.to_owned(),
        driver,
        startup: config.startup.unwrap_or(ManifestContainerStartup::Attached),
        profile,
        compose_source: EffectiveComposeSource::Generated,
        compose_files,
        compose_file_display,
        managed_volumes,
        borrowed_volumes,
        data_peers: data_links.peers,
        data_from: data_links.data_from,
        shared_services,
        project_name,
        primary_service,
        dns_domain: dns_routes.first().map(|route| route.domain.clone()),
        dns_base_domain,
        runtime_host_key: runtime_host_key.map(str::to_owned),
        dns_tls: dns_routes.first().is_some_and(|route| route.tls),
        dns_port: dns_routes.first().and_then(|route| route.port),
        dns_routes,
        service_aliases,
        declared_ports: effective_ports,
        ports_declared_explicitly: !host.ports.is_empty(),
        declared_mounts: resolved_host_mounts,
        declared_media_mounts: data.media,
        pull_production_hook: data.pull_production,
        health_check: health.check,
        health_timeout_secs: health.timeout_secs.unwrap_or(DEFAULT_HEALTH_TIMEOUT_SECS),
        secret_delivery: secrets
            .delivery
            .unwrap_or(ManifestContainerSecretDelivery::ComposeEnv),
        secret_runtime_dir: secrets
            .runtime_dir
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        source_secret_runtime_for_deferrals: secrets.source_for_deferrals.unwrap_or(false),
        workspace_user: workspace
            .and_then(|value| value.user.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| default_workspace_identity.map(|(user, _)| user.to_owned())),
        workspace_home: workspace
            .and_then(|value| value.home.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or_else(|| default_workspace_identity.map(|(_, home)| home.to_owned())),
        on_task_exit: lifecycle
            .and_then(|value| value.on_task_exit)
            .unwrap_or(ManifestContainerOnTaskExit::Stop),
        shutdown: lifecycle
            .and_then(|value| value.shutdown)
            .unwrap_or(ManifestContainerShutdownMode::Graceful),
        detach_timeout_secs: lifecycle
            .and_then(|value| value.detach_timeout_secs)
            .unwrap_or(DEFAULT_ATTACH_TIMEOUT_SECS),
        host_processes: config
            .host_processes
            .iter()
            .map(|entry| resolve_host_process(entry, runtime_host_key))
            .collect(),
    };
    draft.finish(&mut policy, effective_manifest)?;
    // A host-process `env` reference to a resolved environment fact fails here,
    // at policy load, naming the key and the valid names (effigy#321).
    super::host_process_env::validate_host_process_env(&policy)?;
    if let Some(render) = &mut policy.render {
        render.request = super::render::Request::Named(workspace.cloned());
    }
    Ok((policy, workspace_mount_decisions))
}

/// Every `data_from` link touching this environment, in both directions,
/// plus the generated-compose borrow input when this environment borrows.
/// The manifest validates every reference at load; this resolution stays
/// defensive so a future caller cannot skip that validation silently.
struct VolumeDataLinks {
    data_from: Option<String>,
    peers: Vec<EffectiveDataPeer>,
    borrow: Option<ComposeVolumeBorrow>,
}

fn resolve_volume_data_links(
    repo_root: &Path,
    containers: &ManifestContainersConfig,
    default_project_name_base: &str,
    name: &str,
    config: &ManifestContainerConfig,
) -> Result<VolumeDataLinks, ContainerPolicyError> {
    let container_count = containers.environments.len();
    let project_for = |peer_name: &str, peer_config: &ManifestContainerConfig| {
        resolve_project_name(
            peer_config,
            default_project_name_base,
            peer_name,
            container_count,
            repo_root,
        )
    };
    let mut peers = Vec::new();
    let mut data_from = None;
    let mut borrow = None;
    if let Some(owner_name) = config
        .data_from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let Some(owner) = containers.environments.get(owner_name) else {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{name}` declares `data_from = \"{owner_name}\"` but that environment is not defined"
            )));
        };
        let services = shared_data_service_names(config, owner);
        let owner_project = project_for(owner_name, owner)?;
        peers.push(EffectiveDataPeer {
            environment: owner_name.to_owned(),
            project_name: owner_project.clone(),
            services: services.clone(),
        });
        borrow = Some(ComposeVolumeBorrow {
            owner_project_name: owner_project,
            services: services.into_iter().collect(),
        });
        data_from = Some(owner_name.to_owned());
    }
    for (peer_name, peer_config) in &containers.environments {
        let borrows_this = peer_config
            .data_from
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_some_and(|owner| owner == name);
        if !borrows_this {
            continue;
        }
        let services = shared_data_service_names(peer_config, config);
        peers.push(EffectiveDataPeer {
            environment: peer_name.clone(),
            project_name: project_for(peer_name, peer_config)?,
            services,
        });
    }
    peers.sort_by(|left, right| left.environment.cmp(&right.environment));
    Ok(VolumeDataLinks {
        data_from,
        peers,
        borrow,
    })
}

/// Service names declared with the same catalog entry in both environments,
/// excluding host-global `shared = true` services, which live in their own
/// project and are never borrowed. Sorted and deduplicated.
fn shared_data_service_names(
    borrower: &ManifestContainerConfig,
    owner: &ManifestContainerConfig,
) -> Vec<String> {
    let mut names = borrower
        .services
        .iter()
        .filter(|(service_name, service)| {
            if service.shared.unwrap_or(false) {
                return false;
            }
            owner
                .services
                .get(service_name.as_str())
                .is_some_and(|owner_service| {
                    !owner_service.shared.unwrap_or(false)
                        && owner_service.catalog == service.catalog
                })
        })
        .map(|(service_name, _)| service_name.clone())
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}

fn resolve_host_process(
    entry: &effigy_manifest::ManifestContainerHostProcess,
    runtime_host_key: Option<&str>,
) -> EffectiveHostProcess {
    use effigy_manifest::ManifestContainerHostProcessRestart;
    let restart = match entry.restart.unwrap_or_default() {
        ManifestContainerHostProcessRestart::OnFailure => HostProcessRestart::OnFailure,
        ManifestContainerHostProcessRestart::Always => HostProcessRestart::Always,
        ManifestContainerHostProcessRestart::Never => HostProcessRestart::Never,
    };
    let shutdown_signal = entry
        .shutdown_signal
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_uppercase)
        .as_deref()
        .map(parse_host_process_signal)
        .unwrap_or(HostProcessSignal::Sigterm);
    EffectiveHostProcess {
        name: entry.name.trim().to_owned(),
        run: entry.run.clone(),
        cwd: entry
            .cwd
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_default(),
        env: entry
            .env
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
        depends_on: entry.depends_on.clone(),
        listener: entry.listener.as_ref().map(|listener| {
            let declared = listener.route.domain.clone();
            EffectiveManagedHostListener {
                bind: listener.bind.clone(),
                readiness_path: listener.readiness.path.clone(),
                readiness_status: listener.readiness.status,
                readiness_timeout_secs: listener.readiness.timeout_secs,
                route_domain: match runtime_host_key {
                    Some(key) => rewrite_declared_host(&declared, key),
                    None => declared.clone(),
                },
                declared_route_domain: declared,
                route_tls: listener.route.tls,
            }
        }),
        restart,
        restart_delay_ms: entry.restart_delay_ms.unwrap_or(1000),
        shutdown_signal,
        shutdown_grace_secs: entry.shutdown_grace_secs.unwrap_or(5),
    }
}

fn parse_host_process_signal(name: &str) -> HostProcessSignal {
    // Manifest validation already guaranteed the value is one of these,
    // but be defensive: fall back to SIGTERM on anything unexpected.
    match name {
        "SIGTERM" => HostProcessSignal::Sigterm,
        "SIGINT" => HostProcessSignal::Sigint,
        "SIGHUP" => HostProcessSignal::Sighup,
        "SIGKILL" => HostProcessSignal::Sigkill,
        _ => HostProcessSignal::Sigterm,
    }
}

fn default_workspace_identity_for_primary_service(
    config: &ManifestContainerConfig,
    primary_service: &str,
) -> Option<(&'static str, &'static str)> {
    let service = config.services.get(primary_service)?;
    match effigy_catalog::service_name(&service.catalog) {
        "php-fpm" => Some(("dev", "/home/dev")),
        "node" => Some(("node", "/home/node")),
        _ => None,
    }
}

fn effective_service_aliases(
    repo_root: &Path,
    config: &ManifestContainerConfig,
) -> Result<Vec<EffectiveServiceAlias>, ContainerPolicyError> {
    let mut aliases = Vec::new();
    for (service_name, service) in config
        .services
        .iter()
        .filter(|(_name, service)| !service.shared.unwrap_or(false))
    {
        let Some(contract) = resolve_catalog_network_contract(Some(repo_root), &service.catalog)?
        else {
            continue;
        };
        aliases.push(EffectiveServiceAlias {
            service: service_name.clone(),
            domain_label: contract.domain_label,
            container_port: contract.container_port,
        });
    }
    Ok(aliases)
}

/// The container the default `[systems]` workspace binds, when that binding
/// names one. `effigy container` without a name falls back to it, so a repo
/// that declares only `[systems]` resolves the same environment the retired
/// `effigy system` lifecycle command used.
fn default_system_container_name(
    manifest: &effigy_manifest::TaskManifest,
) -> Option<ResolvedWorkspaceContainer> {
    let binding = resolve_task_execution_binding_from_parts(
        None,
        manifest.systems.as_ref(),
        manifest.containers.as_ref(),
        "container",
        &ManifestTask::default(),
    )
    .ok()?;
    let ResolvedTaskExecutionBinding::Workspace(binding) = binding? else {
        return None;
    };
    binding.container
}

pub(crate) fn resolve_container_name(
    manifest: &effigy_manifest::TaskManifest,
    containers: &ManifestContainersConfig,
    requested_name: Option<&str>,
) -> Result<String, ContainerPolicyError> {
    requested_name
        .map(str::to_owned)
        .or_else(|| containers.default.clone())
        .or_else(|| match default_system_container_name(manifest) {
            Some(ResolvedWorkspaceContainer::Named(name)) => Some(name),
            // An inline workspace container has no `[containers]` entry to
            // select, so it cannot answer an omitted container name.
            Some(ResolvedWorkspaceContainer::Inline(_)) | None => None,
        })
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(
                "container name omitted but neither `[containers].default` nor a `[systems].default` workspace container is defined for implicit targeting"
                    .to_owned(),
            )
        })
}

fn resolve_container_exec_working_dir(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    workspace: Option<&ManifestWorkspaceConfig>,
) -> Result<PathBuf, ContainerPolicyError> {
    if let Some(workdir) = workspace
        .and_then(|workspace| workspace.working_dir.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return Ok(PathBuf::from(workdir));
    }
    if let Some(working_dir) = config.working_dir.as_ref() {
        return Ok(PathBuf::from(working_dir));
    }

    let Some(host) = config.host.as_ref() else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` must declare `[containers.{container_name}].working_dir` or a repo-root host mount for CWD mapping"
        )));
    };
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    for mount in &host.mounts {
        let (source, target) = match mount {
            effigy_manifest::ManifestContainerHostMount::Spec(raw) => {
                let mut parts = raw.splitn(3, ':');
                let source = parts.next().unwrap_or_default().trim().to_owned();
                let target = parts.next().unwrap_or_default().trim().to_owned();
                (source, target)
            }
            effigy_manifest::ManifestContainerHostMount::Table(table) => {
                // External mounts can't satisfy "container's CWD must
                // map to the repo root" - they live elsewhere by
                // definition, so skip them here.
                if table.external {
                    continue;
                }
                (
                    table.host.trim().to_owned(),
                    table.container.trim().to_owned(),
                )
            }
        };
        if source.is_empty() || target.is_empty() {
            continue;
        }
        let resolved_source = repo_root.join(&source);
        let canonical_source = resolved_source.canonicalize().unwrap_or(resolved_source);
        if canonical_source == canonical_root {
            return Ok(PathBuf::from(target));
        }
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{container_name}` must declare `[containers.{container_name}].working_dir` or a repo-root host mount for CWD mapping"
    )))
}
