use std::path::PathBuf;

use effigy_catalog::{volumes::ManagedVolume, CatalogError};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerShutdownMode,
    ManifestContainerStartup, ManifestError,
};

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
    pub host_processes: Vec<EffectiveHostProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveHostProcess {
    pub name: String,
    pub run: String,
    pub restart: HostProcessRestart,
    pub restart_delay_ms: u64,
    pub shutdown_signal: HostProcessSignal,
    pub shutdown_grace_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProcessRestart {
    OnFailure,
    Always,
    Never,
}

impl HostProcessRestart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnFailure => "on-failure",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProcessSignal {
    Sigterm,
    Sigint,
    Sighup,
    Sigkill,
}

impl HostProcessSignal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sigterm => "SIGTERM",
            Self::Sigint => "SIGINT",
            Self::Sighup => "SIGHUP",
            Self::Sigkill => "SIGKILL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDnsRoute {
    pub domain: String,
    pub tls: bool,
    pub port: Option<u16>,
    pub service: Option<String>,
    /// External `host:port` target. When set, the gateway registers
    /// this route directly against the listener and skips the
    /// container-service host-port resolution. Mutually exclusive
    /// with `service` at the manifest layer.
    pub target_host: Option<String>,
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
    pub domain_label: String,
    pub project_name: String,
    pub compose_file: PathBuf,
    pub host: String,
    pub host_port: u16,
    pub container_port: u16,
    pub host_env_vars: Vec<String>,
    pub port_env_vars: Vec<String>,
}

impl SharedServiceBinding {
    pub fn standard_env_vars(&self) -> Vec<(String, String)> {
        let port = self.host_port.to_string();
        let mut vars = Vec::new();
        vars.extend(
            self.host_env_vars
                .iter()
                .cloned()
                .map(|name| (name, self.host.clone())),
        );
        vars.extend(
            self.port_env_vars
                .iter()
                .cloned()
                .map(|name| (name, port.clone())),
        );
        vars
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
