use std::path::{Path, PathBuf};

use effigy_manifest::{
    load_task_manifest_with_inspection, resolve_task_execution_binding_from_parts,
    ManifestContainerConfig, ManifestContainerDriver, ManifestContainerOnTaskExit,
    ManifestContainerShutdownMode, ManifestContainerStartup, ManifestContainersConfig,
    ManifestTask, ManifestWorkspaceConfig, ResolvedTaskExecutionBinding,
    ResolvedWorkspaceContainer, TASK_MANIFEST_FILE,
};

use crate::mount_spec::resolve_host_mounts;
use crate::policy::project::{
    default_project_name_base, resolve_project_name, validate_unique_project_names,
};
use crate::policy_support::resolve_compose_source;
use crate::runtime::dns::{materialize_runtime_dns_override, runtime_route_domains};
use crate::workspace::materialize_runtime_workspace_mount_rewrite;
use crate::{
    resolve_catalog_network_contract, DEFAULT_ATTACH_TIMEOUT_SECS, DEFAULT_COLIMA_PROFILE,
    DEFAULT_HEALTH_TIMEOUT_SECS,
};

use super::model::{
    ContainerPolicyError, EffectiveAttachMode, EffectiveComposeSource, EffectiveContainerPolicy,
    EffectiveDnsRoute, EffectiveHostProcess, EffectiveServiceAlias, HostProcessRestart,
    HostProcessSignal,
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
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let inferred_workspace = workspace_override
        .cloned()
        .or_else(|| infer_default_workspace_for_container(&loaded.manifest, requested_name));
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let default_project_name_base = default_project_name_base(&loaded.manifest, repo_root);
    validate_unique_project_names(containers, &default_project_name_base, repo_root)?;
    let name = resolve_container_name(containers, requested_name)?;
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
    build_effective_policy(
        repo_root,
        loaded.bundle_root.as_deref(),
        containers,
        &default_project_name_base,
        &name,
        config,
        &loaded.effective_manifest,
        inferred_workspace.as_ref(),
        &library_mounts,
    )
}

pub fn load_all_container_policies(
    repo_root: &Path,
) -> Result<Vec<EffectiveContainerPolicy>, ContainerPolicyError> {
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let default_project_name_base = default_project_name_base(&loaded.manifest, repo_root);
    validate_unique_project_names(containers, &default_project_name_base, repo_root)?;
    let library_mounts = resolve_library_mounts(&loaded.manifest, loaded.bundle_root.as_deref())?;

    let mut policies = containers
        .environments
        .iter()
        .map(|(name, config)| {
            build_effective_policy(
                repo_root,
                loaded.bundle_root.as_deref(),
                containers,
                &default_project_name_base,
                name,
                config,
                &loaded.effective_manifest,
                infer_default_workspace_for_container(&loaded.manifest, Some(name)).as_ref(),
                &library_mounts,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    policies.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(policies)
}

pub fn load_container_exec_working_dir(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<PathBuf, ContainerPolicyError> {
    let manifest_path = repo_root.join(TASK_MANIFEST_FILE);
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let inferred_workspace =
        infer_default_workspace_for_container(&loaded.manifest, requested_name);
    let containers = loaded.manifest.containers.as_ref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let name = resolve_container_name(containers, requested_name)?;
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

    resolve_container_exec_working_dir(repo_root, &name, config, inferred_workspace.as_ref())
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

fn build_effective_policy(
    repo_root: &Path,
    bundle_root: Option<&Path>,
    containers: &ManifestContainersConfig,
    default_project_name_base: &str,
    name: &str,
    config: &ManifestContainerConfig,
    effective_manifest: &str,
    workspace: Option<&ManifestWorkspaceConfig>,
    library_mounts: &[effigy_manifest::LibraryMount],
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
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
    );
    let (
        mut compose_files,
        compose_file_display,
        managed_volumes,
        effective_ports,
        shared_services,
    ) = resolve_compose_source(
        repo_root,
        bundle_root,
        name,
        config,
        &project_name,
        effective_manifest,
    )?;
    let primary_service = config.primary_service.clone().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` must declare `primary_service`"
        ))
    })?;
    if driver == ManifestContainerDriver::Colima {
        if let Some(workspace) = workspace {
            let working_dir =
                resolve_container_exec_working_dir(repo_root, name, config, Some(workspace))?;
            materialize_runtime_workspace_mount_rewrite(
                repo_root,
                name,
                config,
                workspace,
                &working_dir,
                &primary_service,
                &mut compose_files,
                library_mounts,
            )?;
        }
    }
    let dns = config.dns.as_ref().cloned().unwrap_or_default();
    let mut dns_routes = Vec::new();
    for route in dns.resolved_routes() {
        if route.domain.trim().is_empty() {
            continue;
        }
        dns_routes.push(EffectiveDnsRoute {
            domain: route.domain,
            tls: route.tls.unwrap_or(false),
            port: route.port,
            service: route.service.filter(|value| !value.trim().is_empty()),
            target_host: route
                .target_host
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty()),
        });
    }
    let service_aliases = effective_service_aliases(repo_root, config)?;
    if driver == ManifestContainerDriver::Colima {
        let runtime_routes = runtime_route_domains(&dns_routes, &service_aliases);
        materialize_runtime_dns_override(
            repo_root,
            name,
            &profile,
            &runtime_routes,
            &mut compose_files,
        )?;
    }
    let host = config.host.as_ref().cloned().unwrap_or_default();
    let resolved_host_mounts = resolve_host_mounts(repo_root, name, &host.mounts)?;
    let data = config.data.as_ref().cloned().unwrap_or_default();
    let health = config.health.as_ref().cloned().unwrap_or_default();
    let lifecycle = config.lifecycle.as_ref();
    let default_workspace_identity =
        default_workspace_identity_for_primary_service(config, &primary_service);
    let _ = containers;

    Ok(EffectiveContainerPolicy {
        name: name.to_owned(),
        driver,
        startup: config.startup.unwrap_or(ManifestContainerStartup::Attached),
        profile,
        compose_source: if config.compose_file.is_some() {
            EffectiveComposeSource::Direct
        } else {
            EffectiveComposeSource::Generated
        },
        compose_files,
        compose_file_display,
        managed_volumes,
        shared_services,
        project_name,
        primary_service,
        dns_domain: dns_routes.first().map(|route| route.domain.clone()),
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
            .map(resolve_host_process)
            .collect(),
    })
}

fn resolve_host_process(
    entry: &effigy_manifest::ManifestContainerHostProcess,
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
    match service.catalog.as_str() {
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

fn infer_default_workspace_for_container(
    manifest: &effigy_manifest::TaskManifest,
    requested_container_name: Option<&str>,
) -> Option<ManifestWorkspaceConfig> {
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
    let resolved_name = match binding.container.as_ref() {
        Some(ResolvedWorkspaceContainer::Named(name)) => name.as_str(),
        _ => return None,
    };
    match binding.container {
        Some(ResolvedWorkspaceContainer::Named(_))
            if requested_container_name.is_none_or(|name| name == resolved_name) =>
        {
            Some(binding.workspace_config)
        }
        _ => None,
    }
}

fn resolve_container_name(
    containers: &ManifestContainersConfig,
    requested_name: Option<&str>,
) -> Result<String, ContainerPolicyError> {
    requested_name
        .map(str::to_owned)
        .or_else(|| containers.default.clone())
        .or_else(|| sole_dev_context_container_name(containers))
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(
                "container name omitted but `[containers].default` is not defined and no sole `context = \"dev\"` container is available for implicit targeting"
                    .to_owned(),
            )
        })
}

fn sole_dev_context_container_name(containers: &ManifestContainersConfig) -> Option<String> {
    let mut matches = containers
        .environments
        .iter()
        .filter(|(_, config)| config.context.as_deref() == Some("dev"))
        .map(|(name, _)| name.clone());
    let first = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(first)
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
