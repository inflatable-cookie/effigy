pub mod colima;
pub mod compose;
pub mod exec;
pub mod health;
pub mod report;
pub mod session;

pub use report::{
    down_report, eject_report, logs_report, reset_report, stats_all_report, status_all_report,
    status_report, up_detached_report, AllocatedPortsSummary, ContainerCommandReport,
    ContainerStatsAllEntry, ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
};

use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use effigy_catalog::{
    assembly::ServiceDeclaration, volumes::ManagedVolume, CatalogError, CatalogResolver,
    ComposeAssembler, ComposeOutput,
};
use effigy_gateway::ports::PortRegistry;
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestContainerConfig, ManifestContainerDriver,
    ManifestContainerOnTaskExit, ManifestContainerServiceConfig, ManifestContainerShutdownMode,
    ManifestContainerStartup, ManifestContainersConfig, ManifestError,
};
use serde_yaml::Value as YamlValue;

const DEFAULT_COLIMA_PROFILE: &str = "default";
const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_COMPOSE_DIR: &str = "infra/dev";
const SHARED_SERVICE_HOST: &str = "host.docker.internal";

#[cfg(test)]
thread_local! {
    static TEST_EFFIGY_HOME: std::cell::RefCell<Option<PathBuf>> = const {
        std::cell::RefCell::new(None)
    };
}

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
    pub declared_ports: Vec<String>,
    pub ports_declared_explicitly: bool,
    pub declared_mounts: Vec<String>,
    pub health_check: Option<String>,
    pub health_timeout_secs: u64,
    pub ui_tabs: Vec<String>,
    pub on_task_exit: ManifestContainerOnTaskExit,
    pub shutdown: ManifestContainerShutdownMode,
    pub detach_timeout_secs: u64,
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
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;
    let name = requested_name
        .map(str::to_owned)
        .or_else(|| containers.default.clone())
        .ok_or_else(|| {
            ContainerPolicyError::TaskInvocation(
                "container name omitted but `[containers].default` is not defined".to_owned(),
            )
        })?;
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
        &containers,
        &name,
        config,
        &loaded.effective_manifest,
    )
}

pub fn load_all_container_policies(
    repo_root: &Path,
) -> Result<Vec<EffectiveContainerPolicy>, ContainerPolicyError> {
    let manifest_path = repo_root.join("effigy.toml");
    let loaded = load_task_manifest_with_inspection(&manifest_path)?;
    let containers = loaded.manifest.containers.ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(
            "manifest does not define a `[containers]` registry".to_owned(),
        )
    })?;

    let mut policies = containers
        .environments
        .iter()
        .map(|(name, config)| {
            build_effective_policy(
                repo_root,
                &containers,
                name,
                config,
                &loaded.effective_manifest,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    policies.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(policies)
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
    Ok(())
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
    name: &str,
    config: &ManifestContainerConfig,
    effective_manifest: &str,
) -> Result<EffectiveContainerPolicy, ContainerPolicyError> {
    let project_name = config.project_name.clone().unwrap_or_else(|| {
        let repo = repo_root
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("repo")
            .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
        format!("{repo}-{name}-dev")
    });
    let (compose_files, compose_file_display, managed_volumes, effective_ports, shared_services) =
        resolve_compose_source(repo_root, name, config, &project_name, effective_manifest)?;
    let primary_service = config.primary_service.clone().ok_or_else(|| {
        ContainerPolicyError::TaskInvocation(format!(
            "container `{name}` must declare `primary_service`"
        ))
    })?;
    let dns = config.dns.as_ref().cloned().unwrap_or_default();
    let host = config.host.as_ref().cloned().unwrap_or_default();
    let health = config.health.as_ref().cloned().unwrap_or_default();
    let ui_tabs = config
        .ui
        .as_ref()
        .map(|value| value.tabs.clone())
        .unwrap_or_default();
    let lifecycle = config.lifecycle.as_ref();
    let _ = containers;

    Ok(EffectiveContainerPolicy {
        name: name.to_owned(),
        driver: config.driver.unwrap_or(ManifestContainerDriver::Colima),
        startup: config.startup.unwrap_or(ManifestContainerStartup::Attached),
        profile: config
            .profile
            .clone()
            .unwrap_or_else(|| DEFAULT_COLIMA_PROFILE.to_owned()),
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
        dns_domain: Some(dns.domain).filter(|value| !value.trim().is_empty()),
        dns_tls: dns.tls.unwrap_or(false),
        dns_port: dns.port,
        declared_ports: effective_ports,
        ports_declared_explicitly: !host.ports.is_empty(),
        declared_mounts: host.mounts,
        health_check: health.check,
        health_timeout_secs: health.timeout_secs.unwrap_or(DEFAULT_HEALTH_TIMEOUT_SECS),
        ui_tabs,
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

    let output = ComposeOutput::new(repo_root.join(GENERATED_COMPOSE_DIR));
    let eject = output.eject()?;
    Ok(ContainerEjectResult {
        compose_path: eject.compose_path,
        dockerfile_count: eject.dockerfile_paths.len(),
        config_count: eject.config_paths.len(),
    })
}

fn resolve_compose_source(
    repo_root: &Path,
    container_name: &str,
    config: &ManifestContainerConfig,
    project_name: &str,
    effective_manifest: &str,
) -> Result<
    (
        Vec<PathBuf>,
        String,
        Vec<ManagedVolume>,
        Vec<String>,
        Vec<SharedServiceBinding>,
    ),
    ContainerPolicyError,
> {
    if config.compose_file.is_some() && !config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot combine `compose_file` with `[containers.{container_name}.services]`"
        )));
    }

    if let Some(compose_file) = &config.compose_file {
        let compose_file =
            repo_relative_path(repo_root, compose_file, "containers.*.compose_file")?;
        let display = path_relative_to_repo(repo_root, &compose_file);
        return Ok((
            vec![compose_file],
            display,
            Vec::new(),
            configured_host_ports(config),
            Vec::new(),
        ));
    }

    if config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` must declare either `compose_file` or `[containers.{container_name}.services]`"
        )));
    }

    let shared_services = resolve_shared_service_bindings(config)?;
    let local_services = config
        .services
        .iter()
        .filter(|(_name, service)| !service.shared.unwrap_or(false))
        .map(|(name, service)| (name.clone(), service.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();
    if local_services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot mark every generated service as `shared = true`; at least one local service must remain"
        )));
    }
    if config.primary_service.as_ref().is_some_and(|primary| {
        shared_services
            .iter()
            .any(|service| service.service_name == *primary)
    }) {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot use shared service `{}` as `primary_service`",
            config.primary_service.as_deref().unwrap_or_default()
        )));
    }

    let services = build_service_declarations(repo_root, &local_services)?;
    let resolver = CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    );
    let assembler = ComposeAssembler::new(resolver);
    let mut assembly =
        assembler.assemble(&services, project_name, &repo_root.display().to_string())?;
    apply_shared_service_env_policy(project_name, &shared_services, &mut assembly)?;
    let configured_bindings = configured_host_ports(config)
        .iter()
        .map(|raw| parse_port_binding(raw))
        .collect::<Result<Vec<_>, _>>()?;
    let shared_container_ports = shared_services
        .iter()
        .map(|service| service.container_port)
        .collect::<std::collections::BTreeSet<_>>();
    let conflicting_shared_ports = configured_bindings
        .iter()
        .filter(|binding| shared_container_ports.contains(&binding.container))
        .map(|binding| format!("{}:{}", binding.host, binding.container))
        .collect::<Vec<_>>();
    if !conflicting_shared_ports.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot bind shared service port mappings through `[containers.{container_name}.host].ports`: {}",
            conflicting_shared_ports.join(", ")
        )));
    }
    let effective_ports = apply_generated_compose_port_policy(
        repo_root,
        project_name,
        &configured_bindings,
        &mut assembly,
    )?;
    let output = ComposeOutput::new(repo_root.join(GENERATED_COMPOSE_DIR));
    let manifest_cache_key = if effective_ports.is_empty() {
        effective_manifest.to_owned()
    } else {
        format!(
            "{effective_manifest}\n# effective_ports={}\n# shared_services={}",
            effective_ports.join(","),
            shared_services
                .iter()
                .map(|service| format!(
                    "{}:{}:{}",
                    service.catalog, service.project_name, service.host_port
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let write = output.write(&assembly, &manifest_cache_key)?;

    let mut compose_files = vec![write.compose_path];
    if output.has_override() {
        compose_files.push(output.override_compose_path());
    }
    let display = compose_files
        .iter()
        .map(|path| path_relative_to_repo(repo_root, path))
        .collect::<Vec<_>>()
        .join(", ");
    Ok((
        compose_files,
        display,
        assembly
            .volumes
            .iter()
            .map(ManagedVolume::from_volume_info)
            .collect(),
        effective_ports,
        shared_services,
    ))
}

fn build_service_declarations(
    repo_root: &Path,
    services: &std::collections::BTreeMap<String, ManifestContainerServiceConfig>,
) -> Result<Vec<ServiceDeclaration>, ContainerPolicyError> {
    services
        .iter()
        .map(|(name, service)| {
            Ok(ServiceDeclaration {
                name: name.clone(),
                catalog: service.catalog.clone(),
                params: service
                    .params
                    .iter()
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect(),
                variant: service.variant.clone(),
                config: service
                    .config
                    .as_deref()
                    .map(|raw| repo_relative_path(repo_root, raw, "containers.*.services.*.config"))
                    .transpose()?,
            })
        })
        .collect()
}

fn resolve_shared_service_bindings(
    config: &ManifestContainerConfig,
) -> Result<Vec<SharedServiceBinding>, ContainerPolicyError> {
    let mut bindings = Vec::new();
    for (service_name, service) in &config.services {
        if !service.shared.unwrap_or(false) {
            continue;
        }
        validate_supported_shared_service(service_name, service)?;
        let shared_project_name = shared_service_project_name(service);
        let output_dir = shared_service_output_dir(&shared_project_name)?;
        let declaration = ServiceDeclaration {
            name: service_name.clone(),
            catalog: service.catalog.clone(),
            params: service
                .params
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect(),
            variant: None,
            config: None,
        };
        let resolver = CatalogResolver::new(None, user_global_catalog_dir());
        let assembler = ComposeAssembler::new(resolver);
        let mut assembly = assembler.assemble(
            &[declaration],
            &shared_project_name,
            &output_dir.display().to_string(),
        )?;
        let effective_ports = apply_generated_compose_port_policy(
            &output_dir,
            &shared_project_name,
            &[],
            &mut assembly,
        )?;
        if effective_ports.len() != 1 {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "shared service `{service_name}` must expose exactly one published port on the bounded shared-service path"
            )));
        }
        let binding = parse_port_binding(&effective_ports[0])?;
        let output = ComposeOutput::new(output_dir);
        let write = output.write(&assembly, &shared_project_name)?;
        bindings.push(SharedServiceBinding {
            service_name: service_name.clone(),
            catalog: service.catalog.clone(),
            project_name: shared_project_name,
            compose_file: write.compose_path,
            host: SHARED_SERVICE_HOST.to_owned(),
            host_port: binding.host,
            container_port: binding.container,
        });
    }
    bindings.sort_by(|left, right| left.service_name.cmp(&right.service_name));
    Ok(bindings)
}

fn validate_supported_shared_service(
    service_name: &str,
    service: &ManifestContainerServiceConfig,
) -> Result<(), ContainerPolicyError> {
    match service.catalog.as_str() {
        "mariadb" | "postgres" | "redis" | "memcached" => {}
        other => {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "service `{service_name}` uses unsupported shared catalog `{other}`; the bounded shared-service path only supports `mariadb`, `postgres`, `redis`, and `memcached`"
            )));
        }
    }
    if service.variant.is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "service `{service_name}` cannot combine `shared = true` with `variant`; the bounded shared-service path only supports standalone backing-service catalogs"
        )));
    }
    if service.config.is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "service `{service_name}` cannot combine `shared = true` with `config`; the bounded shared-service path only supports standalone backing-service catalogs"
        )));
    }
    Ok(())
}

fn shared_service_project_name(service: &ManifestContainerServiceConfig) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    service.catalog.hash(&mut hasher);
    service.variant.hash(&mut hasher);
    service.config.hash(&mut hasher);
    for (key, value) in &service.params {
        key.hash(&mut hasher);
        value.to_string().hash(&mut hasher);
    }
    format!(
        "effigy-shared-{}-{:08x}",
        service.catalog,
        (hasher.finish() & 0xffff_ffff) as u32
    )
}

fn shared_service_output_dir(project_name: &str) -> Result<PathBuf, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Err(ContainerPolicyError::TaskInvocation(
            "`HOME` is not set; cannot resolve shared service state directory".to_owned(),
        ));
    };
    Ok(home.join("shared-services").join(project_name))
}

fn apply_shared_service_env_policy(
    project_name: &str,
    shared_services: &[SharedServiceBinding],
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<(), ContainerPolicyError> {
    if shared_services.is_empty() {
        return Ok(());
    }

    let mut parsed: YamlValue =
        serde_yaml::from_str(&assembly.compose_yaml).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before shared-service rewrite: {error}"
            ))
        })?;
    let Some(services) = parsed
        .get_mut("services")
        .and_then(YamlValue::as_mapping_mut)
    else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "generated compose for `{project_name}` is missing a `services` mapping"
        )));
    };

    let env_vars = shared_services
        .iter()
        .flat_map(SharedServiceBinding::standard_env_vars)
        .collect::<Vec<_>>();

    for service in services.values_mut() {
        let Some(service) = service.as_mapping_mut() else {
            continue;
        };
        inject_service_env_vars(service, &env_vars)?;
    }

    assembly.compose_yaml = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize generated compose for `{project_name}` after shared-service rewrite: {error}"
        ))
    })?;
    Ok(())
}

fn inject_service_env_vars(
    service: &mut serde_yaml::Mapping,
    env_vars: &[(String, String)],
) -> Result<(), ContainerPolicyError> {
    let key = YamlValue::String("environment".to_owned());
    let environment = match service.get_mut(&key) {
        Some(value) => value,
        None => {
            service.insert(key.clone(), YamlValue::Mapping(serde_yaml::Mapping::new()));
            service
                .get_mut(&key)
                .expect("environment mapping should exist after insertion")
        }
    };

    let environment = match environment {
        YamlValue::Mapping(mapping) => mapping,
        YamlValue::Sequence(sequence) => {
            let mut mapping = serde_yaml::Mapping::new();
            for entry in sequence.iter() {
                let Some(raw) = entry.as_str() else {
                    return Err(ContainerPolicyError::TaskInvocation(
                        "shared-service env injection only supports string list `environment` entries".to_owned(),
                    ));
                };
                let Some((name, value)) = raw.split_once('=') else {
                    return Err(ContainerPolicyError::TaskInvocation(format!(
                        "shared-service env injection expected `KEY=VALUE` environment entry, got `{raw}`"
                    )));
                };
                mapping.insert(
                    YamlValue::String(name.trim().to_owned()),
                    YamlValue::String(value.trim().to_owned()),
                );
            }
            *environment = YamlValue::Mapping(mapping);
            environment
                .as_mapping_mut()
                .expect("environment mapping should exist after conversion")
        }
        _ => {
            return Err(ContainerPolicyError::TaskInvocation(
                "shared-service env injection only supports mapping or string-list `environment` entries".to_owned(),
            ))
        }
    };

    for (name, value) in env_vars {
        environment
            .entry(YamlValue::String(name.clone()))
            .or_insert_with(|| YamlValue::String(value.clone()));
    }
    Ok(())
}

fn apply_generated_compose_port_policy(
    repo_root: &Path,
    project_name: &str,
    explicit_bindings: &[PortBinding],
    assembly: &mut effigy_catalog::assembly::AssemblyResult,
) -> Result<Vec<String>, ContainerPolicyError> {
    let mut parsed: YamlValue =
        serde_yaml::from_str(&assembly.compose_yaml).map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` is invalid YAML before port policy rewrite: {error}"
            ))
        })?;
    let Some(services) = parsed
        .get_mut("services")
        .and_then(YamlValue::as_mapping_mut)
    else {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "generated compose for `{project_name}` is missing a `services` mapping"
        )));
    };

    let mut used_explicit_ports = std::collections::BTreeSet::<u16>::new();
    let mut effective_ports = Vec::new();

    let mut registry = if explicit_bindings.is_empty() {
        Some(load_port_registry()?.unwrap_or_default())
    } else {
        None
    };

    let mut service_names = services
        .keys()
        .filter_map(YamlValue::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    service_names.sort();

    for service_name in service_names {
        let Some(service) = services
            .get_mut(YamlValue::String(service_name.clone()))
            .and_then(YamlValue::as_mapping_mut)
        else {
            continue;
        };
        let Some(ports) = service
            .get_mut(YamlValue::String("ports".to_owned()))
            .and_then(YamlValue::as_sequence_mut)
        else {
            continue;
        };

        for port in ports.iter_mut() {
            let Some(raw) = port.as_str() else {
                continue;
            };
            let binding = parse_port_binding(raw)?;
            let host_port = if explicit_bindings.is_empty() {
                let registry = registry.as_mut().expect("registry exists");
                registry
                    .assign_port(
                        project_name,
                        &repo_root.display().to_string(),
                        binding.container,
                    )
                    .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))?
            } else {
                let Some(explicit) = explicit_bindings
                    .iter()
                    .find(|candidate| candidate.container == binding.container)
                else {
                    continue;
                };
                used_explicit_ports.insert(explicit.container);
                explicit.host
            };
            *port = YamlValue::String(format!("{host_port}:{}", binding.container));
            effective_ports.push(format!("{host_port}:{}", binding.container));
        }
    }

    if let Some(registry) = registry.as_ref() {
        save_port_registry(registry)?;
    }

    if !explicit_bindings.is_empty() {
        let unused = explicit_bindings
            .iter()
            .filter(|binding| !used_explicit_ports.contains(&binding.container))
            .map(|binding| format!("{}:{}", binding.host, binding.container))
            .collect::<Vec<_>>();
        if !unused.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "generated compose for `{project_name}` does not expose manifest `host.ports` mapping(s): {}",
                unused.join(", ")
            )));
        }
    }

    assembly.compose_yaml = serde_yaml::to_string(&parsed).map_err(|error| {
        ContainerPolicyError::TaskInvocation(format!(
            "failed to serialize generated compose for `{project_name}` after port policy rewrite: {error}"
        ))
    })?;
    Ok(effective_ports)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PortBinding {
    host: u16,
    container: u16,
}

fn parse_port_binding(raw: &str) -> Result<PortBinding, ContainerPolicyError> {
    let mut parts = raw.split(':');
    let host = parts.next().unwrap_or_default().trim();
    let container = parts.next().unwrap_or_default().trim();
    if host.is_empty() || container.is_empty() || parts.next().is_some() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "unsupported port mapping `{raw}`; expected `<host-port>:<container-port>`"
        )));
    }
    Ok(PortBinding {
        host: host.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid host port mapping `{raw}`: {error}"
            ))
        })?,
        container: container.parse::<u16>().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "invalid container port mapping `{raw}`: {error}"
            ))
        })?,
    })
}

fn load_port_registry() -> Result<Option<PortRegistry>, ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(None);
    };
    let path = home.join("ports.json");
    PortRegistry::load(&path)
        .map(Some)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

fn save_port_registry(registry: &PortRegistry) -> Result<(), ContainerPolicyError> {
    let Some(home) = effigy_home_dir() else {
        return Ok(());
    };
    let path = home.join("ports.json");
    registry
        .save(&path)
        .map_err(|error| ContainerPolicyError::TaskInvocation(error.to_string()))
}

fn project_local_catalog_dir(repo_root: &Path) -> Option<PathBuf> {
    let path = repo_root.join(GENERATED_COMPOSE_DIR).join("catalog");
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let path = effigy_home_dir()?.join("catalog");
    path.is_dir().then_some(path)
}

fn effigy_home_dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = test_effigy_home_override() {
        return Some(path);
    }

    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".effigy"))
}

#[cfg(test)]
pub(crate) fn with_test_effigy_home<T>(path: &Path, run: impl FnOnce() -> T) -> T {
    struct ResetGuard(Option<PathBuf>);

    impl Drop for ResetGuard {
        fn drop(&mut self) {
            let previous = self.0.take();
            TEST_EFFIGY_HOME.with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }

    let previous = TEST_EFFIGY_HOME.with(|slot| slot.borrow_mut().replace(path.to_path_buf()));
    let _guard = ResetGuard(previous);
    run()
}

#[cfg(test)]
fn test_effigy_home_override() -> Option<PathBuf> {
    TEST_EFFIGY_HOME.with(|slot| slot.borrow().clone())
}

fn configured_host_ports(config: &ManifestContainerConfig) -> Vec<String> {
    config
        .host
        .as_ref()
        .map(|host| host.ports.clone())
        .unwrap_or_default()
}

fn validate_declared_mounts(
    repo_root: &Path,
    container_name: &str,
    mounts: &[String],
) -> Result<(), ContainerPolicyError> {
    let repo_root = repo_root
        .canonicalize()
        .map_err(|error| ContainerPolicyError::Read {
            path: repo_root.to_path_buf(),
            error,
        })?;
    for mount in mounts {
        let source = mount.split(':').next().unwrap_or_default().trim();
        if source.is_empty() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` has invalid mount `{mount}`; expected `<repo-relative-source>:<target>`"
            )));
        }
        let source_path = Path::new(source);
        if source_path.is_absolute() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` must use a repo-relative source path"
            )));
        }
        let resolved = repo_root.join(source_path);
        let canonical = resolved.canonicalize().map_err(|error| {
            ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount source `{source}` is invalid: {error}"
            ))
        })?;
        if canonical.strip_prefix(&repo_root).is_err() {
            return Err(ContainerPolicyError::TaskInvocation(format!(
                "container `{container_name}` mount `{mount}` escapes the repo root"
            )));
        }
    }
    Ok(())
}

fn repo_relative_path(
    repo_root: &Path,
    raw: &str,
    field: &str,
) -> Result<PathBuf, ContainerPolicyError> {
    let path = Path::new(raw);
    if path.is_absolute() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "`{field}` must stay repo-relative in v1"
        )));
    }
    Ok(repo_root.join(path))
}

fn path_relative_to_repo(repo_root: &Path, path: &Path) -> String {
    path.strip_prefix(repo_root)
        .unwrap_or(path)
        .display()
        .to_string()
}

pub fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests;
