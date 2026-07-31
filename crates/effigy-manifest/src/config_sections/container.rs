use std::collections::BTreeMap;

use super::ManifestIsolationAdoption;

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ManifestContainersConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(flatten)]
    pub environments: BTreeMap<String, ManifestContainerConfig>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
pub struct ManifestSystemsConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(flatten)]
    pub systems: BTreeMap<String, ManifestSystemConfig>,
}

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestSystemConfig {
    #[serde(default)]
    pub default_workspace: Option<String>,
    #[serde(default)]
    pub workspaces: BTreeMap<String, ManifestWorkspaceConfig>,
    #[serde(default)]
    pub container: Option<ManifestWorkspaceContainerRef>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub mounts: Vec<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(default)]
    pub isolation: Vec<ManifestIsolationAdoption>,
}

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ManifestWorkspaceConfig {
    #[serde(default)]
    pub container: Option<ManifestWorkspaceContainerRef>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub mounts: Vec<String>,
    #[serde(default)]
    pub user: Option<String>,
    #[serde(default)]
    pub home: Option<String>,
    #[serde(skip)]
    pub isolation: Vec<ManifestIsolationAdoption>,
}

#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ManifestWorkspaceContainerRef {
    Named(String),
    Inline(ManifestInlineWorkspaceContainerConfig),
}

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq)]
pub struct ManifestInlineWorkspaceContainerConfig {
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub mount: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerConfig {
    #[serde(default)]
    pub driver: Option<ManifestContainerDriver>,
    #[serde(default)]
    pub startup: Option<ManifestContainerStartup>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub compose_file: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    #[serde(default)]
    pub primary_service: Option<String>,
    #[serde(default)]
    pub services: BTreeMap<String, ManifestContainerServiceConfig>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub aliases: BTreeMap<String, ManifestContainerExecAliasConfig>,
    #[serde(default)]
    pub dns: Option<ManifestContainerDnsConfig>,
    #[serde(default)]
    pub lifecycle: Option<ManifestContainerLifecycleConfig>,
    #[serde(default)]
    pub health: Option<ManifestContainerHealthConfig>,
    #[serde(default)]
    pub secrets: Option<ManifestContainerSecretsConfig>,
    #[serde(default)]
    pub host: Option<ManifestContainerHostConfig>,
    #[serde(default)]
    pub data: Option<ManifestContainerDataConfig>,
    /// Host-side processes that follow this container's lifecycle.
    /// Each entry is started after `compose up` succeeds and stopped
    /// before `compose down`. Useful for sidecars that the containerised
    /// app depends on but that must run on the developer's host
    /// (e.g. an `autossh` SSH tunnel that needs the host's ssh_config).
    /// Crashes are restarted per the entry's `restart` policy. Output
    /// streams to `.effigy/runtime/host-processes/<container>/<name>.log`.
    #[serde(default)]
    pub host_processes: Vec<ManifestContainerHostProcess>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerSecretsConfig {
    #[serde(default)]
    pub delivery: Option<ManifestContainerSecretDelivery>,
    #[serde(default)]
    pub runtime_dir: Option<String>,
    #[serde(default)]
    pub source_for_deferrals: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestContainerSecretDelivery {
    ComposeEnv,
    RuntimeFiles,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerDnsConfig {
    #[serde(default)]
    pub routes: Vec<ManifestContainerDnsRouteConfig>,
    /// Sugar: a flat list of domain names that expand into routes
    /// inheriting from `domain_defaults`. Literal entries in `routes`
    /// with the same domain win over their sugar form, so power users
    /// can still override individual entries.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Defaults applied to each `domains[i]` entry when expanded into
    /// a route. Ignored if `domains` is empty.
    #[serde(default)]
    pub domain_defaults: Option<ManifestContainerDnsDomainDefaults>,
}

impl ManifestContainerDnsConfig {
    /// Returns the fully resolved set of DNS routes — literal `routes`
    /// entries followed by any `domains[i]` sugar entries that aren't
    /// already covered by a literal route on the same domain. Sugar
    /// entries inherit `domain_defaults`.
    pub fn resolved_routes(&self) -> Vec<ManifestContainerDnsRouteConfig> {
        let mut resolved = self.routes.clone();
        if self.domains.is_empty() {
            return resolved;
        }
        let defaults = self.domain_defaults.clone().unwrap_or_default();
        for domain in &self.domains {
            let trimmed = domain.trim();
            if trimmed.is_empty() {
                continue;
            }
            if resolved
                .iter()
                .any(|existing| existing.domain.trim() == trimmed)
            {
                continue;
            }
            resolved.push(ManifestContainerDnsRouteConfig {
                domain: domain.clone(),
                tls: defaults.tls,
                port: defaults.port,
                service: defaults.service.clone(),
                target_host: defaults.target_host.clone(),
            });
        }
        resolved
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerDnsDomainDefaults {
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub service: Option<String>,
    /// External target in `host:port` form. When set, the gateway
    /// registers the route directly against this host listener and
    /// skips the container-service resolution. Mutually exclusive
    /// with `service`.
    #[serde(default)]
    pub target_host: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerDnsRouteConfig {
    pub domain: String,
    #[serde(default)]
    pub tls: Option<bool>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub service: Option<String>,
    /// External target in `host:port` form. See `domain_defaults`.
    /// Mutually exclusive with `service` on the same route.
    #[serde(default)]
    pub target_host: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestContainerServiceConfig {
    pub catalog: String,
    #[serde(default)]
    pub variant: Option<String>,
    #[serde(default)]
    pub config: Option<String>,
    #[serde(default)]
    pub shared: Option<bool>,
    #[serde(flatten)]
    pub params: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestContainerExecAliasConfig {
    Service(String),
    Config(ManifestContainerExecAliasTableConfig),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerExecAliasTableConfig {
    pub service: String,
    #[serde(default)]
    pub command: Option<String>,
}

impl ManifestContainerExecAliasConfig {
    pub fn service(&self) -> &str {
        match self {
            Self::Service(service) => service,
            Self::Config(config) => &config.service,
        }
    }

    pub fn command<'a>(&'a self, alias_name: &'a str) -> &'a str {
        match self {
            Self::Service(_) => alias_name,
            Self::Config(config) => config.command.as_deref().unwrap_or(alias_name),
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerDriver {
    Colima,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerStartup {
    Attached,
    Detached,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestContainerOnTaskExit {
    Stop,
    LeaveRunning,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestContainerShutdownMode {
    Graceful,
    Immediate,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerLifecycleConfig {
    #[serde(default)]
    pub on_task_exit: Option<ManifestContainerOnTaskExit>,
    #[serde(default)]
    pub shutdown: Option<ManifestContainerShutdownMode>,
    #[serde(default)]
    pub detach_timeout_secs: Option<u64>,
}

/// A host-side process tied to a container's lifecycle.
///
/// Started after `compose up` for the parent container, stopped before
/// `compose down`. Output is appended to a per-process log file under
/// `.effigy/runtime/host-processes/<container>/<name>.log`; the
/// supervisor PID lives next to it as `<name>.pid`.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHostProcess {
    /// Stable identifier for this process. Used for log/PID file names
    /// and shutdown lookup. Must be unique within the container's
    /// `host_processes` list and contain only `[A-Za-z0-9_-]` characters.
    pub name: String,
    /// Host shell command to execute. Runs under `sh -lc <run>`.
    pub run: String,
    /// Restart policy when the process exits. Defaults to `on-failure`
    /// (restart only when the exit code is non-zero). `always` restarts
    /// regardless; `never` exits the supervisor on first exit.
    #[serde(default)]
    pub restart: Option<ManifestContainerHostProcessRestart>,
    /// Delay between restart attempts, in milliseconds. Defaults to 1000.
    #[serde(default)]
    pub restart_delay_ms: Option<u64>,
    /// Signal sent during graceful shutdown. Defaults to `SIGTERM`.
    /// Accepts `SIGTERM`, `SIGINT`, `SIGHUP`, or `SIGKILL`.
    #[serde(default)]
    pub shutdown_signal: Option<String>,
    /// Seconds to wait after the shutdown signal before escalating to
    /// `SIGKILL`. Defaults to 5.
    #[serde(default)]
    pub shutdown_grace_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestContainerHostProcessRestart {
    #[default]
    OnFailure,
    Always,
    Never,
}

impl ManifestContainerHostProcessRestart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::OnFailure => "on-failure",
            Self::Always => "always",
            Self::Never => "never",
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHealthConfig {
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHostConfig {
    #[serde(default)]
    pub ports: Vec<String>,
    #[serde(default)]
    pub mounts: Vec<ManifestContainerHostMount>,
    /// Host address used when publishing generated compose ports.
    /// Defaults to `127.0.0.1` so generated services are only reachable
    /// from the local machine. Set to `0.0.0.0` to publish on all
    /// interfaces (previous behavior).
    #[serde(default)]
    pub publish_address: Option<String>,
}

/// A host -> container bind mount declaration on `[containers.<name>.host]`.
///
/// Two forms accepted at the manifest layer:
///
/// - **Legacy string form** — a colon-separated `host:container[:options]`
///   spec. Source must be repo-relative; absolute and `~`-prefixed paths
///   are rejected.
/// - **Structured form** — a table that opts into out-of-repo sources via
///   `external = true` and supports `${VAR}` / `~` expansion in `host`.
///
/// Both forms render down to the same internal mount string before
/// reaching the compose layer.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestContainerHostMount {
    /// `"host:container[:options]"` — legacy form, repo-relative only.
    Spec(String),
    /// Structured form. Use `external = true` to source the mount from
    /// outside the repo root (with `${VAR}` / `~` expansion in `host`).
    Table(ManifestContainerHostMountTable),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerHostMountTable {
    /// Host-side path. Supports `${VAR}` (process env) and `~` expansion.
    /// Without `external = true`, must resolve to a repo-relative path
    /// under the manifest root; with `external = true`, may live
    /// anywhere on disk.
    pub host: String,
    /// Container-side mount target. Absolute path inside the container.
    pub container: String,
    /// Opt-in: source the mount from outside the repo root. Required
    /// to use absolute paths, `~` expansion, or `${VAR}` references
    /// that resolve outside the repo. Defaults to false.
    #[serde(default)]
    pub external: bool,
    /// Extra option tokens (e.g. `["ro"]`) appended after the container
    /// path in the rendered mount spec. Reserved for future use; the
    /// container layer passes them through unchanged today.
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestContainerDataConfig {
    #[serde(default)]
    pub media: Vec<String>,
    #[serde(default)]
    pub pull_production: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestDataConfig {
    #[serde(default)]
    pub targets: BTreeMap<String, ManifestDataTargetConfig>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDataTargetConfig {
    pub service: String,
    pub database: String,
}
