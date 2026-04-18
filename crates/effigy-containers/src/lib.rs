pub mod colima;
pub mod compose;
pub mod exec;
pub mod health;
pub mod report;
pub mod session;

pub use report::{
    down_report, eject_report, logs_report, reset_report, status_all_report, status_report,
    up_detached_report, AllocatedPortsSummary, ContainerCommandReport, ContainerStatusAllEntry,
    ContainerStatusService,
};

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use effigy_catalog::{
    assembly::ServiceDeclaration, CatalogError, CatalogResolver, ComposeAssembler, ComposeOutput,
};
use effigy_manifest::{
    load_task_manifest_with_inspection, ManifestContainerConfig, ManifestContainerDriver,
    ManifestContainerOnTaskExit, ManifestContainerServiceConfig, ManifestContainerShutdownMode,
    ManifestContainerStartup, ManifestContainersConfig, ManifestError,
};

const DEFAULT_COLIMA_PROFILE: &str = "default";
const DEFAULT_ATTACH_TIMEOUT_SECS: u64 = 10;
const DEFAULT_HEALTH_TIMEOUT_SECS: u64 = 60;
const GENERATED_COMPOSE_DIR: &str = "infra/dev";

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
    pub project_name: String,
    pub primary_service: String,
    pub dns_domain: Option<String>,
    pub dns_tls: bool,
    pub dns_port: Option<u16>,
    pub declared_ports: Vec<String>,
    pub declared_mounts: Vec<String>,
    pub health_check: Option<String>,
    pub health_timeout_secs: u64,
    pub ui_tabs: Vec<String>,
    pub on_task_exit: ManifestContainerOnTaskExit,
    pub shutdown: ManifestContainerShutdownMode,
    pub detach_timeout_secs: u64,
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
    let (compose_files, compose_file_display) =
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
        project_name,
        primary_service,
        dns_domain: Some(dns.domain).filter(|value| !value.trim().is_empty()),
        dns_tls: dns.tls.unwrap_or(false),
        dns_port: dns.port,
        declared_ports: host.ports,
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
) -> Result<(Vec<PathBuf>, String), ContainerPolicyError> {
    if config.compose_file.is_some() && !config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` cannot combine `compose_file` with `[containers.{container_name}.services]`"
        )));
    }

    if let Some(compose_file) = &config.compose_file {
        let compose_file =
            repo_relative_path(repo_root, compose_file, "containers.*.compose_file")?;
        let display = path_relative_to_repo(repo_root, &compose_file);
        return Ok((vec![compose_file], display));
    }

    if config.services.is_empty() {
        return Err(ContainerPolicyError::TaskInvocation(format!(
            "container `{container_name}` must declare either `compose_file` or `[containers.{container_name}.services]`"
        )));
    }

    let services = build_service_declarations(repo_root, &config.services)?;
    let resolver = CatalogResolver::new(
        project_local_catalog_dir(repo_root),
        user_global_catalog_dir(),
    );
    let assembler = ComposeAssembler::new(resolver);
    let assembly = assembler.assemble(&services, project_name, &repo_root.display().to_string())?;
    let output = ComposeOutput::new(repo_root.join(GENERATED_COMPOSE_DIR));
    let write = output.write(&assembly, effective_manifest)?;

    let mut compose_files = vec![write.compose_path];
    if output.has_override() {
        compose_files.push(output.override_compose_path());
    }
    let display = compose_files
        .iter()
        .map(|path| path_relative_to_repo(repo_root, path))
        .collect::<Vec<_>>()
        .join(", ");
    Ok((compose_files, display))
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

fn project_local_catalog_dir(repo_root: &Path) -> Option<PathBuf> {
    let path = repo_root.join(GENERATED_COMPOSE_DIR).join("catalog");
    path.is_dir().then_some(path)
}

fn user_global_catalog_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(".effigy").join("catalog");
    path.is_dir().then_some(path)
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
