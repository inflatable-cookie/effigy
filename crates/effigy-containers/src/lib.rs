pub mod colima;
pub mod compose;
pub mod exec;
pub mod health;
mod policy_support;
pub mod report;
pub mod session;

pub use report::{
    data_list_report, data_pull_production_report, data_transfer_report, down_report, eject_report,
    logs_report, reset_report, stats_all_report, status_all_report, status_report,
    up_detached_report, AllocatedPortsSummary, ContainerCommandReport, ContainerDataHookResult,
    ContainerDataTransferAction, ContainerDataVolumeEntry, ContainerStatsAllEntry,
    ContainerStatsService, ContainerStatusAllEntry, ContainerStatusService,
};

#[cfg(test)]
pub(crate) use policy_support::with_test_effigy_home;
use policy_support::{resolve_compose_source, validate_declared_mounts, validate_media_mounts};

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_catalog::{volumes::ManagedVolume, CatalogError, ComposeOutput};
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestContainerConfig, ManifestContainerDriver,
    ManifestContainerOnTaskExit, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestError,
};

const DEFAULT_COLIMA_PROFILE: &str = "default";
const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_COMPOSE_DIR: &str = "infra/dev";
const SHARED_SERVICE_HOST: &str = "host.docker.internal";

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
    pub declared_media_mounts: Vec<String>,
    pub pull_production_hook: Option<String>,
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
    validate_media_mounts(repo_root, &policy.name, &policy.declared_media_mounts)?;
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
    let data = config.data.as_ref().cloned().unwrap_or_default();
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
        declared_media_mounts: data.media,
        pull_production_hook: data.pull_production,
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

pub fn driver_label(driver: ManifestContainerDriver) -> &'static str {
    match driver {
        ManifestContainerDriver::Colima => "colima",
    }
}

#[cfg(test)]
mod tests;
