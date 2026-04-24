pub mod colima;
pub mod compose;
pub mod exec;
pub mod health;
mod policy_support;
pub mod report;
pub mod session;
mod workspace;

pub use report::{
    data_list_report, data_pull_production_report, data_transfer_report, down_report, eject_report,
    logs_report, reset_report, stats_all_report, status_all_report, status_report,
    up_detached_report, AllocatedPortsSummary, ContainerCommandReport, ContainerDataHookResult,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerStatsAllEntry,
    ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
};
pub use workspace::load_workspace_ownership_targets;

#[cfg(test)]
pub(crate) use compose::with_test_compose_backend;
#[cfg(test)]
pub(crate) use policy_support::with_test_effigy_home;
use policy_support::{resolve_compose_source, validate_declared_mounts, validate_media_mounts};
use workspace::materialize_runtime_workspace_mount_rewrite;
#[cfg(test)]
pub(crate) use workspace::with_test_host_composer_home;

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_catalog::{volumes::ManagedVolume, CatalogError, ComposeOutput};
use effigy_core::runtime_dir::ensure_effigy_ignored_in_git_root;
use effigy_manifest::{
    load_task_manifest_with_inspection, resolve_task_execution_binding_from_parts,
    ManifestContainerConfig, ManifestContainerDriver, ManifestContainerOnTaskExit,
    ManifestContainerShutdownMode, ManifestContainerStartup, ManifestContainersConfig,
    ManifestError, ManifestInlineWorkspaceContainerConfig, ManifestTask, ManifestWorkspaceConfig,
    ResolvedTaskExecutionBinding, ResolvedWorkspaceContainer, TASK_MANIFEST_FILE,
};

pub(crate) const DEFAULT_COLIMA_PROFILE: &str = "effigy";
const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_RUNTIME_COMPOSE_DIR: &str = ".effigy/runtime/compose";
const PROJECT_LOCAL_CATALOG_DIR: &str = "infra/dev/catalog";
const EJECTED_COMPOSE_DIR: &str = "infra/dev";
const SHARED_SERVICE_HOST: &str = "host.docker.internal";
const RUNTIME_DNS_FALLBACK_SERVERS: [&str; 2] = ["1.1.1.1", "8.8.8.8"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveAttachMode {
    Attached,
    Detached,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveComposeSource {
    Direct,
    Generated,
}

#[derive(Debug, Clone)]
pub struct EffectiveContainerPolicy {
    pub name: String,
    pub driver: ManifestContainerDriver,
    pub startup: ManifestContainerStartup,
    pub profile: String,
    pub compose_source: EffectiveComposeSource,
    pub compose_files: Vec<PathBuf>,
    pub compose_file_display: String,
    pub managed_volumes: Vec<ManagedVolume>,
    pub shared_services: Vec<SharedServiceBinding>,
    pub project_name: String,
    pub primary_service: String,
    pub dns_domain: Option<String>,
    pub dns_tls: bool,
    pub dns_port: Option<u16>,
    pub dns_routes: Vec<EffectiveDnsRoute>,
    pub service_aliases: Vec<EffectiveServiceAlias>,
    pub declared_ports: Vec<String>,
    pub ports_declared_explicitly: bool,
    pub declared_mounts: Vec<String>,
    pub declared_media_mounts: Vec<String>,
    pub pull_production_hook: Option<String>,
    pub health_check: Option<String>,
    pub health_timeout_secs: u64,
    pub workspace_user: Option<String>,
    pub workspace_home: Option<String>,
    pub on_task_exit: ManifestContainerOnTaskExit,
    pub shutdown: ManifestContainerShutdownMode,
    pub detach_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDnsRoute {
    pub domain: String,
    pub tls: bool,
    pub port: Option<u16>,
    pub service: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveServiceAlias {
    pub service: String,
    pub domain_label: String,
    pub container_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedServiceBinding {
    pub service_name: String,
    pub catalog: String,
    pub project_name: String,
    pub compose_file: PathBuf,
    pub host: String,
    pub host_port: u16,
    pub container_port: u16,
}

pub fn service_alias_contract(catalog: &str) -> Option<(&'static str, u16)> {
    match catalog {
        "postgres" => Some(("postgres", 5432)),
        "mariadb" | "mysql" => Some(("mysql", 3306)),
        "redis" => Some(("redis", 6379)),
        "memcached" => Some(("memcached", 11211)),
        "elasticsearch" => Some(("search", 9200)),
        "minio" | "s3" => Some(("s3", 9000)),
        "mail" | "mailpit" => Some(("smtp", 1025)),
        _ => None,
    }
}

impl SharedServiceBinding {
    pub fn standard_env_vars(&self) -> Vec<(String, String)> {
        let port = self.host_port.to_string();
        match self.catalog.as_str() {
            "mariadb" => vec![
                ("DB_HOST".to_owned(), self.host.clone()),
                ("DB_PORT".to_owned(), port.clone()),
                ("MYSQL_HOST".to_owned(), self.host.clone()),
                ("MYSQL_PORT".to_owned(), port),
            ],
            "postgres" => vec![
                ("POSTGRES_HOST".to_owned(), self.host.clone()),
                ("POSTGRES_PORT".to_owned(), port.clone()),
                ("PGHOST".to_owned(), self.host.clone()),
                ("PGPORT".to_owned(), port),
            ],
            "redis" => vec![
                ("REDIS_HOST".to_owned(), self.host.clone()),
                ("REDIS_PORT".to_owned(), port),
            ],
            "memcached" => vec![
                ("MEMCACHED_HOST".to_owned(), self.host.clone()),
                ("MEMCACHED_PORT".to_owned(), port),
            ],
            _ => Vec::new(),
        }
    }
}

#[derive(Debug)]
pub enum ContainerPolicyError {
    Manifest(ManifestError),
    Catalog(CatalogError),
    TaskInvocation(String),
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerEjectResult {
    pub compose_path: PathBuf,
    pub dockerfile_count: usize,
    pub config_count: usize,
}

impl std::fmt::Display for ContainerPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(error) => write!(f, "{error}"),
            Self::Catalog(error) => write!(f, "{error}"),
            Self::TaskInvocation(message) => write!(f, "{message}"),
            Self::Read { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
        }
    }
}

impl std::error::Error for ContainerPolicyError {}

impl From<ManifestError> for ContainerPolicyError {
    fn from(value: ManifestError) -> Self {
        Self::Manifest(value)
    }
}

impl From<CatalogError> for ContainerPolicyError {
    fn from(value: CatalogError) -> Self {
        Self::Catalog(value)
    }
}

pub fn load_container_policy(
    repo_root: &Path,
    requested_name: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    load_container_policy_with_workspace(repo_root, requested_name, None)
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
    validate_unique_project_names(containers, &default_project_name_base)?;
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
    build_effective_policy(
        repo_root,
        containers,
        &default_project_name_base,
        &name,
        config,
        &loaded.effective_manifest,
        inferred_workspace.as_ref(),
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
    validate_unique_project_names(containers, &default_project_name_base)?;

    let mut policies = containers
        .environments
        .iter()
        .map(|(name, config)| {
            build_effective_policy(
                repo_root,
                containers,
                &default_project_name_base,
                name,
                config,
                &loaded.effective_manifest,
                infer_default_workspace_for_container(&loaded.manifest, Some(name)).as_ref(),
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

pub fn load_inline_workspace_container_policy(
    repo_root: &Path,
    synthetic_name: &str,
    container: &ManifestInlineWorkspaceContainerConfig,
    workdir: Option<&str>,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let image = container.image.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` must declare `image`"
        ))
    })?;
    let compose_dir = repo_root
        .join(".effigy")
        .join("inline-workspaces")
        .join(synthetic_name);
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&compose_dir).map_err(|error| ContainerPolicyError::Read {
        path: compose_dir.clone(),
        error,
    })?;
    let compose_path = compose_dir.join("docker-compose.yml");
    let effective_workdir =
        resolve_inline_workspace_exec_working_dir(repo_root, synthetic_name, container, workdir)?;
    let volume_mount = container
        .mount
        .as_deref()
        .map(|mount| inline_workspace_compose_mount(repo_root, synthetic_name, mount))
        .transpose()?;
    let compose = render_inline_workspace_compose(
        image,
        effective_workdir.as_path(),
        volume_mount.as_deref(),
    );
    std::fs::write(&compose_path, compose).map_err(|error| ContainerPolicyError::Read {
        path: compose_path.clone(),
        error,
    })?;
    let repo = repo_root
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("repo")
        .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
    let mut compose_files = vec![compose_path.clone()];
    materialize_runtime_dns_override(
        repo_root,
        synthetic_name,
        DEFAULT_COLIMA_PROFILE,
        &RuntimeDnsOverrideRoutes::default(),
        &mut compose_files,
    )?;
    Ok(EffectiveContainerPolicy {
        name: synthetic_name.to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Attached,
        profile: DEFAULT_COLIMA_PROFILE.to_owned(),
        compose_source: EffectiveComposeSource::Direct,
        compose_files,
        compose_file_display: compose_path
            .strip_prefix(repo_root)
            .unwrap_or(&compose_path)
            .display()
            .to_string(),
        managed_volumes: Vec::new(),
        shared_services: Vec::new(),
        project_name: format!("{repo}-{synthetic_name}-inline"),
        primary_service: "workspace".to_owned(),
        dns_domain: None,
        dns_tls: false,
        dns_port: None,
        dns_routes: Vec::new(),
        service_aliases: Vec::new(),
        declared_ports: Vec::new(),
        ports_declared_explicitly: false,
        declared_mounts: container.mount.clone().into_iter().collect(),
        declared_media_mounts: Vec::new(),
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: DEFAULT_HEALTH_TIMEOUT_SECS,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: DEFAULT_ATTACH_TIMEOUT_SECS,
    })
}

pub fn resolve_inline_workspace_exec_working_dir(
    repo_root: &Path,
    synthetic_name: &str,
    container: &ManifestInlineWorkspaceContainerConfig,
    workdir: Option<&str>,
) -> Result<PathBuf, ContainerPolicyError> {
    if let Some(workdir) = workdir.map(str::trim).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(workdir));
    }
    let mount = container.mount.as_deref().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` must declare `mount` or workspace `working_dir` for exec CWD mapping"
        ))
    })?;
    let (_source, target, _options) = parse_mount_parts(mount).ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must use `source:target` form"
        ))
    })?;
    if target.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must declare a non-empty target path"
        )));
    }
    let _ = repo_root;
    Ok(PathBuf::from(target))
}

pub fn validate_container_policy(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if policy.driver != ManifestContainerDriver::Colima {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` uses unsupported driver `{}`; v1 only supports `colima`",
            policy.name,
            driver_label(policy.driver)
        )));
    }
    for compose_file in &policy.compose_files {
        if !compose_file.is_file() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{}` compose_file not found: {}",
                policy.name,
                compose_file.display()
            )));
        }
    }
    validate_declared_mounts(repo_root, &policy.name, &policy.declared_mounts)?;
    validate_media_mounts(repo_root, &policy.name, &policy.declared_media_mounts)?;
    Ok(())
}

/// Verify the resolved compose backend can actually reach the repo host paths.
///
/// This is the runtime preflight that rejects Colima-nerdctl fallback runs
/// against repos living under temp directories the Colima VM may not share.
/// It is NOT part of static manifest validation — call it only from code
/// paths that will actually drive `docker compose`.
pub fn validate_compose_backend_runtime(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    validate_compose_backend_host_paths(repo_root, policy)
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
    containers: &ManifestContainersConfig,
    default_project_name_base: &str,
    name: &str,
    config: &ManifestContainerConfig,
    effective_manifest: &str,
    workspace: Option<&ManifestWorkspaceConfig>,
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
    );
    let (
        mut compose_files,
        compose_file_display,
        managed_volumes,
        effective_ports,
        shared_services,
    ) = resolve_compose_source(repo_root, name, config, &project_name, effective_manifest)?;
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
            )?;
        }
    }
    let dns = config.dns.as_ref().cloned().unwrap_or_default();
    let mut dns_routes = Vec::new();
    for route in dns.routes {
        if route.domain.trim().is_empty() {
            continue;
        }
        dns_routes.push(EffectiveDnsRoute {
            domain: route.domain,
            tls: route.tls.unwrap_or(false),
            port: route.port,
            service: route.service.filter(|value| !value.trim().is_empty()),
        });
    }
    let service_aliases = effective_service_aliases(config);
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
        declared_mounts: host.mounts,
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
    })
}

fn validate_compose_backend_host_paths(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<(), ContainerPolicyError> {
    if compose::resolve_compose_backend() != compose::ComposeBackend::ColimaNerdctl {
        return Ok(());
    }
    if std::env::var_os("EFFIGY_TEST_SKIP_COLIMA_TEMP_ROOT_CHECK").is_some()
        || std::env::var_os("EFFIGY_TEST_COLIMA_ARGS_FILE").is_some()
    {
        return Ok(());
    }
    if !is_colima_temp_root_path(repo_root)
        && !policy
            .compose_files
            .iter()
            .any(|compose_file| is_colima_temp_root_path(compose_file))
    {
        return Ok(());
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{}` uses the Colima nerdctl compose fallback, but repo `{}` is under a temp directory that Colima may not share into the VM; move the repo under a shared path like `/Users/...` and retry",
        policy.name,
        repo_root.display()
    )))
}

fn is_colima_temp_root_path(path: &Path) -> bool {
    let temp_root = std::env::temp_dir();
    path_is_within(path, &temp_root)
        || path_is_within(path, Path::new("/tmp"))
        || path_is_within(path, Path::new("/private/tmp"))
        || path_is_within(path, Path::new("/var/folders"))
        || path_is_within(path, Path::new("/private/var/folders"))
}

fn path_is_within(path: &Path, root: &Path) -> bool {
    path.starts_with(root)
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

fn effective_service_aliases(config: &ManifestContainerConfig) -> Vec<EffectiveServiceAlias> {
    config
        .services
        .iter()
        .filter(|(_name, service)| !service.shared.unwrap_or(false))
        .filter_map(|(service_name, service)| {
            service_alias_contract(&service.catalog).map(|(domain_label, container_port)| {
                EffectiveServiceAlias {
                    service: service_name.clone(),
                    domain_label: domain_label.to_owned(),
                    container_port,
                }
            })
        })
        .collect()
}

fn default_project_name_base(manifest: &effigy_manifest::TaskManifest, repo_root: &Path) -> String {
    manifest
        .catalog
        .as_ref()
        .and_then(|catalog| catalog.alias.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(sanitize_project_name_component)
        .unwrap_or_else(|| {
            let repo = repo_root
                .file_name()
                .and_then(OsStr::to_str)
                .unwrap_or("repo");
            sanitize_project_name_component(repo)
        })
}

fn sanitize_project_name_component(value: &str) -> String {
    value.replace(|c: char| !c.is_ascii_alphanumeric(), "-")
}

fn resolve_project_name(
    config: &ManifestContainerConfig,
    default_project_name_base: &str,
    name: &str,
    container_count: usize,
) -> String {
    config
        .project_name
        .clone()
        .unwrap_or_else(|| default_project_name(default_project_name_base, name, container_count))
}

fn default_project_name(
    default_project_name_base: &str,
    name: &str,
    container_count: usize,
) -> String {
    if container_count <= 1 {
        return format!("{default_project_name_base}-dev");
    }
    format!("{default_project_name_base}-{name}-dev")
}

fn validate_unique_project_names(
    containers: &ManifestContainersConfig,
    default_project_name_base: &str,
) -> Result<(), ContainerPolicyError> {
    if containers.environments.len() <= 1 {
        return Ok(());
    }

    let mut by_project_name: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, config) in &containers.environments {
        by_project_name
            .entry(resolve_project_name(
                config,
                default_project_name_base,
                name,
                containers.environments.len(),
            ))
            .or_default()
            .push(name.clone());
    }

    let duplicates = by_project_name
        .into_iter()
        .filter(|(_project_name, names)| names.len() > 1)
        .map(|(project_name, mut names)| {
            names.sort();
            format!(
                "`{project_name}` for containers {}",
                names
                    .into_iter()
                    .map(|name| format!("`{name}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        return Ok(());
    }

    Err(ContainerPolicyError::TaskInvocation(format!(
        "containers must resolve to unique `project_name` values when more than one container is declared; duplicate effective project names: {}",
        duplicates.join("; ")
    )))
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
        let mut parts = mount.splitn(3, ':');
        let source = parts.next().unwrap_or_default().trim();
        let target = parts.next().unwrap_or_default().trim();
        if target.is_empty() {
            continue;
        }
        let resolved_source = repo_root.join(source);
        let canonical_source = resolved_source.canonicalize().unwrap_or(resolved_source);
        if canonical_source == canonical_root {
            return Ok(PathBuf::from(target));
        }
    }
    Err(ContainerPolicyError::TaskInvocation(format!(
        "container `{container_name}` must declare `[containers.{container_name}].working_dir` or a repo-root host mount for CWD mapping"
    )))
}

fn inline_workspace_compose_mount(
    repo_root: &Path,
    synthetic_name: &str,
    mount: &str,
) -> Result<String, ContainerPolicyError> {
    let (source, target, options) = parse_mount_parts(mount).ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount `{mount}` must use `source:target` form"
        ))
    })?;
    let resolved_source = repo_root.join(source);
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    let canonical_source = resolved_source
        .canonicalize()
        .unwrap_or_else(|_| resolved_source.clone());
    if !canonical_source.starts_with(&canonical_root) {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "inline workspace container `{synthetic_name}` mount source `{source}` escapes the repo root"
        )));
    }
    let mut rendered = format!("{}:{target}", canonical_source.display());
    if let Some(options) = options.filter(|value| !value.is_empty()) {
        rendered.push(':');
        rendered.push_str(options);
    }
    Ok(rendered)
}

fn parse_mount_parts(mount: &str) -> Option<(&str, &str, Option<&str>)> {
    let mut parts = mount.splitn(3, ':');
    let source = parts.next()?.trim();
    let target = parts.next()?.trim();
    let options = parts.next().map(str::trim);
    Some((source, target, options))
}

fn render_inline_workspace_compose(
    image: &str,
    workdir: &Path,
    volume_mount: Option<&str>,
) -> String {
    let workdir = workdir.display().to_string();
    let mut out = String::new();
    out.push_str("services:\n");
    out.push_str("  workspace:\n");
    out.push_str(&format!("    image: \"{}\"\n", image.replace('"', "\\\"")));
    out.push_str(&format!(
        "    working_dir: \"{}\"\n",
        workdir.replace('"', "\\\"")
    ));
    out.push_str("    command:\n");
    out.push_str("      - sh\n");
    out.push_str("      - -lc\n");
    out.push_str("      - while true; do sleep 3600; done\n");
    if let Some(volume_mount) = volume_mount {
        out.push_str("    volumes:\n");
        out.push_str(&format!(
            "      - \"{}\"\n",
            volume_mount.replace('"', "\\\"")
        ));
    }
    out
}

fn materialize_runtime_dns_override(
    repo_root: &Path,
    container_name: &str,
    profile: &str,
    routes: &RuntimeDnsOverrideRoutes,
    compose_files: &mut Vec<PathBuf>,
) -> Result<(), ContainerPolicyError> {
    let services = collect_compose_service_names(compose_files)?;
    if services.is_empty() {
        return Ok(());
    }
    let dns_servers = resolve_runtime_dns_servers(profile);
    let gateway_address = resolve_runtime_gateway_address(profile);
    if dns_servers.is_empty() && routes.is_empty() {
        return Ok(());
    }
    let override_dir = repo_root.join(".effigy").join("runtime").join("dns");
    ensure_effigy_ignored_in_git_root(repo_root).map_err(|error| ContainerPolicyError::Read {
        path: repo_root.join(".gitignore"),
        error,
    })?;
    std::fs::create_dir_all(&override_dir).map_err(|error| ContainerPolicyError::Read {
        path: override_dir.clone(),
        error,
    })?;
    let override_path = override_dir.join(format!("{container_name}.compose.override.yml"));
    let override_yaml =
        render_runtime_dns_override(&services, &dns_servers, gateway_address.as_deref(), routes);
    std::fs::write(&override_path, override_yaml).map_err(|error| ContainerPolicyError::Read {
        path: override_path.clone(),
        error,
    })?;
    compose_files.push(override_path);
    Ok(())
}

fn collect_compose_service_names(
    compose_files: &[PathBuf],
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut names = std::collections::BTreeSet::new();
    for compose_file in compose_files {
        let content =
            std::fs::read_to_string(compose_file).map_err(|error| ContainerPolicyError::Read {
                path: compose_file.clone(),
                error,
            })?;
        let parsed: serde_yaml::Value = serde_yaml::from_str(&content).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "failed to parse compose file {} for runtime DNS override generation: {error}",
                compose_file.display()
            ))
        })?;
        let Some(services) = parsed
            .get("services")
            .and_then(serde_yaml::Value::as_mapping)
        else {
            continue;
        };
        for key in services.keys() {
            if let Some(name) = key.as_str() {
                names.insert(name.to_owned());
            }
        }
    }
    Ok(names.into_iter().collect())
}

fn resolve_runtime_dns_servers(profile: &str) -> Vec<String> {
    let Some(colima_home) = colima_home_dir() else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let config_path = colima_home.join(profile).join("colima.yaml");
    let Ok(content) = std::fs::read_to_string(&config_path) else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&content) else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let Some(dns) = parsed
        .get("network")
        .and_then(|network| network.get("dns"))
        .and_then(serde_yaml::Value::as_sequence)
    else {
        return RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect();
    };
    let resolved = dns
        .iter()
        .filter_map(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if resolved.is_empty() {
        RUNTIME_DNS_FALLBACK_SERVERS
            .iter()
            .map(|server| (*server).to_owned())
            .collect()
    } else {
        resolved
    }
}

fn resolve_runtime_gateway_address(profile: &str) -> Option<String> {
    let colima_home = colima_home_dir()?;
    let config_path = colima_home.join(profile).join("colima.yaml");
    let content = std::fs::read_to_string(&config_path).ok()?;
    let parsed = serde_yaml::from_str::<serde_yaml::Value>(&content).ok()?;
    parsed
        .get("network")
        .and_then(|network| network.get("gatewayAddress"))
        .and_then(serde_yaml::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .or_else(|| Some("192.168.5.2".to_owned()))
}

fn colima_home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("COLIMA_HOME").map(PathBuf::from) {
        return Some(home);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".colima"))
}

#[derive(Debug, Default)]
struct RuntimeDnsOverrideRoutes {
    host_gateway_domains: Vec<String>,
    service_alias_domains: BTreeMap<String, Vec<String>>,
}

impl RuntimeDnsOverrideRoutes {
    fn is_empty(&self) -> bool {
        self.host_gateway_domains.is_empty() && self.service_alias_domains.is_empty()
    }
}

fn runtime_route_domains(
    dns_routes: &[EffectiveDnsRoute],
    service_aliases: &[EffectiveServiceAlias],
) -> RuntimeDnsOverrideRoutes {
    let mut host_gateway_domains = std::collections::BTreeSet::new();
    let mut service_alias_domains = BTreeMap::<String, Vec<String>>::new();
    let base_domain = dns_routes
        .first()
        .and_then(|route| base_domain_from_route(&route.domain));

    for route in dns_routes {
        let domain = route.domain.trim();
        if !domain.is_empty() {
            host_gateway_domains.insert(domain.to_owned());
        }
    }

    if let Some(base_domain) = base_domain {
        let explicit_domains = dns_routes
            .iter()
            .map(|route| route.domain.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        for alias in service_aliases {
            let domain = format!("{}.{}", alias.domain_label, base_domain);
            if !explicit_domains.contains(domain.as_str()) {
                service_alias_domains
                    .entry(alias.service.clone())
                    .or_default()
                    .push(domain);
            }
        }
    }

    RuntimeDnsOverrideRoutes {
        host_gateway_domains: host_gateway_domains.into_iter().collect(),
        service_alias_domains,
    }
}

fn base_domain_from_route(domain: &str) -> Option<&str> {
    let domain = domain.trim();
    if domain.is_empty() {
        return None;
    }
    let mut labels = domain.split('.').filter(|part| !part.is_empty());
    labels.next()?;
    labels.next()?;
    Some(domain)
}

fn render_runtime_dns_override(
    services: &[String],
    dns_servers: &[String],
    gateway_address: Option<&str>,
    routes: &RuntimeDnsOverrideRoutes,
) -> String {
    let mut out = String::new();
    out.push_str("services:\n");
    for service in services {
        out.push_str(&format!("  {service}:\n"));
        if !dns_servers.is_empty() {
            out.push_str("    dns:\n");
            for server in dns_servers {
                out.push_str(&format!("      - \"{}\"\n", server.replace('"', "\\\"")));
            }
        }
        if let Some(gateway_address) =
            gateway_address.filter(|_| !routes.host_gateway_domains.is_empty())
        {
            out.push_str("    extra_hosts:\n");
            for domain in &routes.host_gateway_domains {
                out.push_str(&format!(
                    "      - \"{}:{}\"\n",
                    domain.replace('"', "\\\""),
                    gateway_address.replace('"', "\\\"")
                ));
            }
        }
        if let Some(domains) = routes.service_alias_domains.get(service) {
            out.push_str("    networks:\n");
            out.push_str("      default:\n");
            out.push_str("        aliases:\n");
            for domain in domains {
                out.push_str(&format!(
                    "          - \"{}\"\n",
                    domain.replace('"', "\\\"")
                ));
            }
        }
    }
    out
}

pub fn eject_generated_compose(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
) -> Result<ContainerEjectResult, ContainerPolicyError> {
    if policy.compose_source != EffectiveComposeSource::Generated {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{}` already uses direct `compose_file` ownership; `eject` only applies to catalog-backed generated compose",
            policy.name
        )));
    }

    let output = ComposeOutput::new(repo_root.join(GENERATED_RUNTIME_COMPOSE_DIR));
    let eject = output.eject_to(&repo_root.join(EJECTED_COMPOSE_DIR))?;
    Ok(ContainerEjectResult {
        compose_path: eject.compose_path,
        dockerfile_count: eject.dockerfile_paths.len(),
        config_count: eject.config_paths.len(),
    })
}

pub fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests;
