use std::path::PathBuf;

use effigy_catalog::{volumes::ManagedVolume, CatalogError};
use effigy_manifest::{
    ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
    ManifestContainerShutdownMode, ManifestContainerStartup, ManifestError,
};
use serde::{Deserialize, Serialize};

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
    pub render: Option<super::render::Rendered>,
    pub repo_root: PathBuf,
    pub name: String,
    pub driver: ManifestContainerDriver,
    pub startup: ManifestContainerStartup,
    pub profile: String,
    pub compose_source: EffectiveComposeSource,
    pub compose_files: Vec<PathBuf>,
    pub compose_file_display: String,
    pub managed_volumes: Vec<ManagedVolume>,
    /// Named volumes this environment mounts from a `data_from` owner's
    /// Compose project. They are declared `external` in the generated
    /// Compose so no Effigy or Compose path ever creates, deletes or claims
    /// them; retirement records list them as retained, never owned.
    pub borrowed_volumes: Vec<ManagedVolume>,
    /// Container environments sharing this environment's service data
    /// volumes through `data_from`, in whichever direction, with the
    /// services whose volumes are shared.
    pub data_peers: Vec<EffectiveDataPeer>,
    /// The environment this one borrows service data volumes from, as
    /// declared by `data_from`.
    pub data_from: Option<String>,
    pub shared_services: Vec<SharedServiceBinding>,
    pub project_name: String,
    pub primary_service: String,
    pub dns_domain: Option<String>,
    pub dns_base_domain: Option<String>,
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
    pub secret_delivery: ManifestContainerSecretDelivery,
    pub secret_runtime_dir: Option<String>,
    pub source_secret_runtime_for_deferrals: bool,
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
    pub cwd: PathBuf,
    pub env: Vec<(String, String)>,
    pub depends_on: Vec<String>,
    pub listener: Option<EffectiveManagedHostListener>,
    pub restart: HostProcessRestart,
    pub restart_delay_ms: u64,
    pub shutdown_signal: HostProcessSignal,
    pub shutdown_grace_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct EffectiveManagedHostListener {
    pub bind: String,
    pub readiness_path: String,
    pub readiness_status: u16,
    pub readiness_timeout_secs: u64,
    /// The manifest's declared host, before checkout scoping.
    pub declared_route_domain: String,
    /// The effective host for this checkout: the declared host for primary
    /// and explicit shared-identity checkouts, otherwise the scoped apex.
    pub route_domain: String,
    pub route_tls: bool,
}

/// How the supervisor replaces a generation that ended by itself. The value is
/// the manifest's own spelling, so a run spec carries it as it was declared.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HostProcessRestart {
    #[default]
    OnFailure,
    Always,
    Never,
}

impl HostProcessRestart {
    /// Whether one completed generation is replaced under this policy. A child
    /// that took a signal is reported as `128 + signal`, so it counts as a
    /// failure under `on-failure`; `always` replaces a clean exit too, and
    /// `never` settles whatever the code.
    pub fn restarts_after(self, code: i32) -> bool {
        match self {
            Self::Always => true,
            Self::OnFailure => code != 0,
            Self::Never => false,
        }
    }
}

/// The signal a supervisor delivers before the kernel's own TERM and grace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HostProcessSignal {
    #[default]
    #[serde(rename = "SIGTERM")]
    Sigterm,
    #[serde(rename = "SIGINT")]
    Sigint,
    #[serde(rename = "SIGHUP")]
    Sighup,
    #[serde(rename = "SIGKILL")]
    Sigkill,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDnsRoute {
    pub domain: String,
    /// Manifest domain before worktree host rewriting. Empty means `domain`.
    pub declared_domain: String,
    pub tls: bool,
    pub port: Option<u16>,
    pub service: Option<String>,
    /// External `host:port` target. When set, the gateway registers
    /// this route directly against the listener and skips the
    /// container-service host-port resolution. Mutually exclusive
    /// with `service` at the manifest layer.
    pub target_host: Option<String>,
}

impl EffectiveDnsRoute {
    pub fn declared(&self) -> &str {
        if self.declared_domain.is_empty() {
            &self.domain
        } else {
            &self.declared_domain
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveServiceAlias {
    pub service: String,
    pub domain_label: String,
    pub container_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveDataPeer {
    /// The peer environment's manifest name.
    pub environment: String,
    /// The peer environment's resolved Compose project name in this
    /// checkout and worktree scope.
    pub project_name: String,
    /// Service names declared with the same catalog entry in both
    /// environments; their named volumes are shared, sorted.
    pub services: Vec<String>,
}

/// Generated-compose rewrite input when this environment borrows another
/// environment's service data volumes through `data_from`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposeVolumeBorrow {
    /// The owner environment's resolved Compose project name in this
    /// checkout and worktree scope. Borrowed volume names are resolved
    /// against it, so a linked worktree borrows the owner's
    /// worktree-scoped volumes, never the primary checkout's.
    pub owner_project_name: String,
    /// Borrowing services with the same name and catalog entry in the
    /// owner environment, sorted.
    pub services: std::collections::BTreeSet<String>,
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

#[cfg(test)]
pub(crate) fn test_policy(repo_root: &std::path::Path) -> EffectiveContainerPolicy {
    use effigy_manifest::{
        ManifestContainerDriver, ManifestContainerOnTaskExit, ManifestContainerSecretDelivery,
        ManifestContainerShutdownMode, ManifestContainerStartup,
    };

    EffectiveContainerPolicy {
        render: None,
        repo_root: repo_root.to_path_buf(),
        name: "hybrid".to_owned(),
        driver: ManifestContainerDriver::Colima,
        startup: ManifestContainerStartup::Detached,
        profile: "effigy".to_owned(),
        compose_source: EffectiveComposeSource::Generated,
        compose_files: vec![],
        compose_file_display: "generated".to_owned(),
        managed_volumes: vec![],
        borrowed_volumes: vec![],
        data_peers: vec![],
        data_from: None,
        shared_services: vec![],
        project_name: "app-dev-wt-abcdef012345".to_owned(),
        primary_service: "app".to_owned(),
        dns_domain: Some("app-wabcdef01.test".to_owned()),
        dns_base_domain: None,
        dns_tls: true,
        dns_port: Some(80),
        dns_routes: vec![EffectiveDnsRoute {
            domain: "app-wabcdef01.test".to_owned(),
            declared_domain: "app.test".to_owned(),
            tls: true,
            port: Some(80),
            service: Some("web".to_owned()),
            target_host: None,
        }],
        service_aliases: vec![],
        declared_ports: vec![],
        ports_declared_explicitly: false,
        declared_mounts: vec![],
        declared_media_mounts: vec![],
        pull_production_hook: None,
        health_check: None,
        health_timeout_secs: 60,
        secret_delivery: ManifestContainerSecretDelivery::ComposeEnv,
        secret_runtime_dir: None,
        source_secret_runtime_for_deferrals: false,
        workspace_user: None,
        workspace_home: None,
        on_task_exit: ManifestContainerOnTaskExit::Stop,
        shutdown: ManifestContainerShutdownMode::Graceful,
        detach_timeout_secs: 10,
        host_processes: Vec::new(),
    }
}

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
