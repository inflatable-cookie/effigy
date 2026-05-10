use std::collections::BTreeMap;
use std::path::Path;

use crate::ManifestError;
use crate::{ManifestManagedRun, ManifestTaskRunIn};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestBundleBase {
    Shipped { name: String },
    Path { dir: String },
    Git { url: String, r#ref: Option<String> },
    Oci { url: String },
}

impl ManifestBundleBase {
    pub fn shipped_name(&self) -> Option<&str> {
        match self {
            Self::Shipped { name } => Some(name.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ManifestBundleConfig {
    pub base: Option<ManifestBundleBase>,
    pub inputs: BTreeMap<String, toml::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
enum ManifestBundleBaseRepr {
    ShippedName(String),
    Typed(ManifestBundleBaseTable),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
enum ManifestBundleBaseTable {
    Shipped {
        name: String,
    },
    Path {
        dir: String,
    },
    Git {
        url: String,
        #[serde(default)]
        r#ref: Option<String>,
    },
    Oci {
        url: String,
    },
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
struct ManifestBundleConfigRepr {
    #[serde(default)]
    base: Option<ManifestBundleBaseRepr>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    base_path: Option<String>,
    #[serde(flatten)]
    inputs: BTreeMap<String, toml::Value>,
}

impl From<ManifestBundleBaseRepr> for ManifestBundleBase {
    fn from(value: ManifestBundleBaseRepr) -> Self {
        match value {
            ManifestBundleBaseRepr::ShippedName(name) => Self::Shipped { name },
            ManifestBundleBaseRepr::Typed(table) => match table {
                ManifestBundleBaseTable::Shipped { name } => Self::Shipped { name },
                ManifestBundleBaseTable::Path { dir } => Self::Path { dir },
                ManifestBundleBaseTable::Git { url, r#ref } => Self::Git { url, r#ref },
                ManifestBundleBaseTable::Oci { url } => Self::Oci { url },
            },
        }
    }
}

impl<'de> serde::Deserialize<'de> for ManifestBundleConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let repr = ManifestBundleConfigRepr::deserialize(deserializer)?;
        if repr.base_path.is_some() {
            return Err(serde::de::Error::custom(
                "`[bundle].base_path` has been removed. Use `base = { type = \"path\", dir = \"...\" }` instead.",
            ));
        }

        let base = match (repr.base, repr.name) {
            (Some(_), Some(_)) => {
                return Err(serde::de::Error::custom(
                    "`[bundle]` cannot set both `base` and legacy `name`",
                ));
            }
            (Some(base), None) => Some(base.into()),
            (None, Some(name)) => Some(ManifestBundleBase::Shipped { name }),
            (None, None) => None,
        };

        Ok(Self {
            base,
            inputs: repr.inputs,
        })
    }
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestTaskDefaultsConfig {
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestIsolationConfig {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, serde::Deserialize, Default, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestIsolationAdoption {
    pub repo: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestShellConfig {
    #[serde(default)]
    pub run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestScanConfig {
    #[serde(default)]
    pub god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub duplicate_blocks: Option<ManifestDuplicateBlocksConfig>,
    #[serde(default)]
    pub comment_ratio: Option<ManifestCommentRatioConfig>,
    #[serde(default)]
    pub generated_assets: Option<ManifestGeneratedAssetsConfig>,
    #[serde(default)]
    pub generated_in_src: Option<ManifestGeneratedInSrcConfig>,
    #[serde(default)]
    pub attention_markers: Option<ManifestAttentionMarkersConfig>,
    #[serde(default)]
    pub stale_suppressions: Option<ManifestStaleSuppressionsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestDuplicateBlocksConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub min_occurrences: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestCommentRatioConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<f64>,
    #[serde(default)]
    pub high: Option<f64>,
    #[serde(default)]
    pub critical: Option<f64>,
    #[serde(default)]
    pub min_code_lines: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestGeneratedInSrcConfig {
    #[serde(default, alias = "threshold")]
    pub warn: Option<usize>,
    #[serde(default)]
    pub warn_bytes: Option<usize>,
    #[serde(default)]
    pub high: Option<usize>,
    #[serde(default)]
    pub high_bytes: Option<usize>,
    #[serde(default)]
    pub critical: Option<usize>,
    #[serde(default)]
    pub critical_bytes: Option<usize>,
    #[serde(default)]
    pub source_root: Option<String>,
    #[serde(default)]
    pub source_roots: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestAttentionMarkersConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestStaleSuppressionsConfig {
    #[serde(default)]
    pub warning: Vec<String>,
    #[serde(default)]
    pub high: Vec<String>,
    #[serde(default)]
    pub critical: Vec<String>,
    #[serde(default)]
    pub fail_on_findings: Option<bool>,
    #[serde(default)]
    pub respect_gitignore: Option<bool>,
    #[serde(default)]
    pub doctor: Option<bool>,
    #[serde(default)]
    pub include: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

impl ManifestJsPackageManager {
    /// Binary name to look up in `PATH` for this manager.
    ///
    /// Returns `None` for `Direct`, which assumes the runner binary
    /// (`vitest` and friends) is already resolvable without an outer
    /// package-manager invocation.
    pub fn binary_name(self) -> Option<&'static str> {
        match self {
            Self::Bun => Some("bun"),
            Self::Pnpm => Some("pnpm"),
            Self::Npm => Some("npm"),
            Self::Direct => None,
        }
    }

    /// Install command for this manager when Effigy needs to hydrate a
    /// JS workspace before running tools from `node_modules/.bin`.
    pub fn install_command(self) -> Option<&'static str> {
        match self {
            Self::Bun => Some("bun install"),
            Self::Pnpm => Some("pnpm install"),
            Self::Npm => Some("npm install"),
            Self::Direct => None,
        }
    }

    /// Vitest invocation for this manager as a `(command, label)` pair.
    ///
    /// The label is used as stable evidence text in plan output, e.g.
    /// `package_manager.js=pnpm`.
    pub fn vitest_command(self) -> (&'static str, &'static str) {
        match self {
            Self::Bun => ("bun x vitest run", "bun"),
            Self::Pnpm => ("pnpm exec vitest run", "pnpm"),
            Self::Npm => ("npx vitest run", "npm"),
            Self::Direct => ("vitest run", "direct"),
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestEnvSchemaConfig {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub schema: Option<String>,
    #[serde(default)]
    pub exec_timeout: Option<u64>,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct ManifestContainersConfig {
    #[serde(default)]
    pub default: Option<String>,
    #[serde(flatten)]
    pub environments: BTreeMap<String, ManifestContainerConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
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
pub struct ManifestContainerConfig {
    #[serde(default)]
    pub driver: Option<ManifestContainerDriver>,
    #[serde(default)]
    pub startup: Option<ManifestContainerStartup>,
    #[serde(default)]
    pub context: Option<String>,
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

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyConfig {
    #[serde(default)]
    pub indexes: BTreeMap<String, ManifestDocsPolicyIndexConfig>,
    #[serde(default, alias = "next_actions")]
    pub next_actions: BTreeMap<String, ManifestDocsPolicyNextActionConfig>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDemoMode {
    Headless,
    Interactive,
    Hybrid,
}

impl ManifestDemoMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Interactive => "interactive",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ManifestDemoStatus {
    Planned,
    Ready,
    Running,
    Passed,
    Failed,
    Broken,
    Missing,
}

impl ManifestDemoStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Broken => "broken",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDemoConfig {
    pub title: String,
    pub summary: String,
    pub proof: String,
    pub owner: String,
    pub mode: ManifestDemoMode,
    pub status: ManifestDemoStatus,
    pub covers: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub artifacts: Vec<String>,
    #[serde(default)]
    pub receipt: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub prerequisites: Vec<String>,
    #[serde(default, alias = "depends_on")]
    pub dependencies: Vec<String>,
}

impl ManifestDemoConfig {
    pub fn validate(&self, manifest_path: &Path, demo_id: &str) -> Result<(), ManifestError> {
        validate_non_empty_demo_string(manifest_path, demo_id, "title", &self.title)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "summary", &self.summary)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "proof", &self.proof)?;
        validate_non_empty_demo_string(manifest_path, demo_id, "owner", &self.owner)?;

        if self.covers.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "invalid `[demos.{demo_id}]`: `covers` must contain at least one coverage key"
                ),
            });
        }

        validate_demo_string_list(manifest_path, demo_id, "covers", &self.covers)?;
        validate_demo_string_list(manifest_path, demo_id, "tags", &self.tags)?;
        validate_demo_string_list(manifest_path, demo_id, "artifacts", &self.artifacts)?;
        validate_demo_string_list(manifest_path, demo_id, "prerequisites", &self.prerequisites)?;
        validate_demo_string_list(manifest_path, demo_id, "dependencies", &self.dependencies)?;

        if let Some(receipt) = &self.receipt {
            validate_non_empty_demo_string(manifest_path, demo_id, "receipt", receipt)?;
        }

        match (&self.task, &self.run) {
            (Some(task), None) => {
                validate_non_empty_demo_string(manifest_path, demo_id, "task", task)?;
            }
            (None, Some(run)) => {
                validate_demo_run(manifest_path, demo_id, run)?;
            }
            (Some(_), Some(_)) | (None, None) => {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "invalid `[demos.{demo_id}]`: declare exactly one runnable entrypoint via `task` or `run`"
                    ),
                });
            }
        }

        Ok(())
    }
}

fn validate_demo_run(
    manifest_path: &Path,
    demo_id: &str,
    run: &ManifestManagedRun,
) -> Result<(), ManifestError> {
    match run {
        ManifestManagedRun::Command(command) => {
            validate_non_empty_demo_string(manifest_path, demo_id, "run", command)
        }
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "invalid `[demos.{demo_id}]`: `run` sequence must contain at least one step"
                    ),
                });
            }
            Ok(())
        }
    }
}

fn validate_non_empty_demo_string(
    manifest_path: &Path,
    demo_id: &str,
    field: &str,
    value: &str,
) -> Result<(), ManifestError> {
    if value.trim().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("invalid `[demos.{demo_id}]`: `{field}` must be a non-empty string"),
        });
    }
    Ok(())
}

fn validate_demo_string_list(
    manifest_path: &Path,
    demo_id: &str,
    field: &str,
    values: &[String],
) -> Result<(), ManifestError> {
    for (index, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "invalid `[demos.{demo_id}]`: `{field}[{index}]` must be a non-empty string"
                ),
            });
        }
    }
    Ok(())
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyIndexConfig {
    pub file: String,
    pub dir: String,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDocsPolicyNextActionConfig {
    pub index: String,
    pub heading: String,
    #[serde(alias = "allowlist_file")]
    pub allowlist_file: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapConfig {
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub start: Option<ManifestBootstrapStart>,
    #[serde(default)]
    pub submodules: Option<ManifestBootstrapSubmodulesPolicy>,
    #[serde(default)]
    pub children: Vec<ManifestBootstrapChildConfig>,
}

/// Selector(s) to run as the bootstrap start task.
///
/// Accepts a single selector string, or an array of entries where each
/// entry is either a selector string (`"dev"`) or a table form
/// (`{ task = "dev" }`). Mixed arrays are allowed, mirroring the
/// flexibility of `[bootstrap].run`. Arrays run sequentially in
/// declaration order; the first failure aborts the chain.
/// Backward-compatible with the original scalar shape.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStart {
    Single(String),
    Multiple(Vec<ManifestBootstrapStartEntry>),
}

/// One entry in a multi-selector `[bootstrap].start` array.
///
/// Either a bare selector string (`"dev"`) or a table form carrying a
/// `task` selector (`{ task = "dev" }`). Args still travel inside the
/// selector string itself (e.g. `"dev --foo bar"`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestBootstrapStartEntry {
    Selector(String),
    Table(ManifestBootstrapStartTable),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapStartTable {
    pub task: String,
}

impl ManifestBootstrapStartEntry {
    pub fn selector(&self) -> &str {
        match self {
            Self::Selector(selector) => selector.as_str(),
            Self::Table(table) => table.task.as_str(),
        }
    }
}

impl ManifestBootstrapStart {
    /// Selectors in declaration order.
    pub fn selectors(&self) -> Vec<&str> {
        match self {
            Self::Single(selector) => vec![selector.as_str()],
            Self::Multiple(entries) => entries.iter().map(|entry| entry.selector()).collect(),
        }
    }

    /// Owned copy of all selectors in declaration order.
    pub fn to_owned_selectors(&self) -> Vec<String> {
        self.selectors().into_iter().map(String::from).collect()
    }
}

#[derive(Debug, serde::Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestBootstrapSubmodulesPolicy {
    None,
    Init,
    Recursive,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestBootstrapChildConfig {
    pub path: String,
    pub repo: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default, alias = "setup")]
    pub run: Option<ManifestManagedRun>,
    #[serde(default = "default_bootstrap_child_required")]
    pub required: bool,
}

fn default_bootstrap_child_required() -> bool {
    true
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionConfig {
    #[serde(default)]
    pub package: Option<ManifestDistributionPackageConfig>,
    #[serde(default)]
    pub publish: Option<ManifestDistributionPublishConfig>,
    #[serde(default)]
    pub preflight: Option<ManifestDistributionPreflightConfig>,
    #[serde(default)]
    pub metadata: Option<ManifestDistributionMetadataConfig>,
    #[serde(default)]
    pub closeout: Option<ManifestDistributionCloseoutConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPackageConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub repo_url: Option<String>,
    #[serde(default)]
    pub brew_formula: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPublishConfig {
    #[serde(default)]
    pub binary_name: Option<String>,
    #[serde(default)]
    pub registry_label: Option<String>,
    #[serde(default)]
    pub verify_tag_install: Option<bool>,
    #[serde(default)]
    pub verify_binary_json_tasks: Option<bool>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionPreflightConfig {
    #[serde(default)]
    pub docs_task: Option<String>,
    #[serde(default)]
    pub smoke_task: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionMetadataConfig {
    #[serde(default)]
    pub required_docs: Option<Vec<String>>,
    #[serde(default)]
    pub required_files: Option<Vec<String>>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestDistributionCloseoutConfig {
    #[serde(default)]
    pub owner: Option<String>,
    #[serde(default)]
    pub related: Option<String>,
    #[serde(default)]
    pub next_step: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseConfig {
    #[serde(default)]
    pub version_file: Option<String>,
    #[serde(default)]
    pub version_path: Option<String>,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default, rename = "pre-1-0")]
    pub pre_1_0: Option<bool>,
    #[serde(default)]
    pub sync_files: Vec<String>,
    #[serde(default)]
    pub gates: BTreeMap<String, ManifestReleaseGateConfig>,
    #[serde(default)]
    pub tag_format: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestReleaseGateConfig {
    Command(String),
    Detailed(ManifestReleaseGateDetails),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestReleaseGateDetails {
    pub command: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        ManifestContainerDnsConfig, ManifestContainerHostConfig, ManifestContainerHostMount,
        ManifestContainerServiceConfig, ManifestContainersConfig,
        ManifestInlineWorkspaceContainerConfig, ManifestIsolationConfig, ManifestJsPackageManager,
        ManifestSystemsConfig, ManifestWorkspaceContainerRef,
    };

    #[derive(Debug, serde::Deserialize)]
    struct ContainerWrapper {
        containers: ManifestContainersConfig,
    }

    #[derive(Debug, serde::Deserialize)]
    struct SystemWrapper {
        systems: ManifestSystemsConfig,
    }

    #[derive(Debug, serde::Deserialize)]
    struct IsolationWrapper {
        isolation: ManifestIsolationConfig,
    }

    #[test]
    fn js_package_manager_binary_names_are_stable() {
        assert_eq!(ManifestJsPackageManager::Bun.binary_name(), Some("bun"));
        assert_eq!(ManifestJsPackageManager::Pnpm.binary_name(), Some("pnpm"));
        assert_eq!(ManifestJsPackageManager::Npm.binary_name(), Some("npm"));
        assert_eq!(ManifestJsPackageManager::Direct.binary_name(), None);
    }

    #[test]
    fn js_package_manager_vitest_commands_are_stable() {
        assert_eq!(
            ManifestJsPackageManager::Bun.vitest_command(),
            ("bun x vitest run", "bun")
        );
        assert_eq!(
            ManifestJsPackageManager::Pnpm.vitest_command(),
            ("pnpm exec vitest run", "pnpm")
        );
        assert_eq!(
            ManifestJsPackageManager::Npm.vitest_command(),
            ("npx vitest run", "npm")
        );
        assert_eq!(
            ManifestJsPackageManager::Direct.vitest_command(),
            ("vitest run", "direct")
        );
    }

    #[test]
    fn js_package_manager_install_commands_are_stable() {
        assert_eq!(
            ManifestJsPackageManager::Bun.install_command(),
            Some("bun install")
        );
        assert_eq!(
            ManifestJsPackageManager::Pnpm.install_command(),
            Some("pnpm install")
        );
        assert_eq!(
            ManifestJsPackageManager::Npm.install_command(),
            Some("npm install")
        );
        assert_eq!(ManifestJsPackageManager::Direct.install_command(), None);
    }

    #[test]
    fn containers_config_accepts_catalog_backed_services() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
context = "dev"
primary_service = "app"

[containers.web.services.app]
catalog = "php-fpm"
version = "8.3"
extensions = ["pdo_mysql", "redis"]

[containers.web.services.web]
catalog = "nginx"
variant = "laravel"

[containers.web.services.db]
catalog = "mariadb"
version = "10.11"
"#,
        )
        .expect("parse containers");

        let web = parsed
            .containers
            .environments
            .get("web")
            .expect("web container");
        assert_eq!(web.context.as_deref(), Some("dev"));
        assert!(web.compose_file.is_none());
        assert_eq!(web.primary_service.as_deref(), Some("app"));

        let app = web.services.get("app").expect("app service");
        assert_eq!(app.catalog, "php-fpm");
        assert_eq!(
            app.params.get("version"),
            Some(&toml::Value::String("8.3".to_string()))
        );
        assert_eq!(
            app.params.get("extensions"),
            Some(&toml::Value::Array(vec![
                toml::Value::String("pdo_mysql".to_string()),
                toml::Value::String("redis".to_string()),
            ]))
        );

        let web_service = web.services.get("web").expect("web service");
        assert_eq!(web_service.catalog, "nginx");
        assert_eq!(web_service.variant.as_deref(), Some("laravel"));
    }

    #[test]
    fn container_service_config_preserves_flattened_params() {
        let parsed: ManifestContainerServiceConfig = toml::from_str(
            r#"
catalog = "redis"
shared = true
memory = 128
enabled = true
"#,
        )
        .expect("parse service");

        assert_eq!(parsed.catalog, "redis");
        assert_eq!(parsed.variant, None);
        assert_eq!(parsed.shared, Some(true));
        assert_eq!(
            parsed.params.get("memory"),
            Some(&toml::Value::Integer(128))
        );
        assert_eq!(
            parsed.params.get("enabled"),
            Some(&toml::Value::Boolean(true))
        );
    }

    #[test]
    fn container_config_accepts_context_with_direct_compose_file() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "dev"

[containers.dev]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
"#,
        )
        .expect("parse containers");

        let dev = parsed
            .containers
            .environments
            .get("dev")
            .expect("dev container");
        assert_eq!(dev.context.as_deref(), Some("dev"));
        assert_eq!(
            dev.compose_file.as_deref(),
            Some("infra/dev/docker-compose.yml")
        );
    }

    #[test]
    fn container_config_accepts_exec_working_dir_and_aliases() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
context = "dev"
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"
working_dir = "/var/www/html"

[containers.web.aliases]
mysql = "db"
artisan = { service = "app", command = "php artisan" }
"#,
        )
        .expect("parse containers");

        let web = parsed
            .containers
            .environments
            .get("web")
            .expect("web container");
        assert_eq!(web.working_dir.as_deref(), Some("/var/www/html"));
        let mysql = web.aliases.get("mysql").expect("mysql alias");
        assert_eq!(mysql.service(), "db");
        assert_eq!(mysql.command("mysql"), "mysql");
        let artisan = web.aliases.get("artisan").expect("artisan alias");
        assert_eq!(artisan.service(), "app");
        assert_eq!(artisan.command("artisan"), "php artisan");
    }

    #[test]
    fn systems_config_accepts_system_level_workspace_config_and_overrides() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
user = "dev"
home = "/home/dev"

[systems.dev.workspaces.app]
working_dir = "/workspace-root/app"
mounts = ["../underlay", "../poodle:/workspace-root/poodle"]
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(
            system
                .container
                .as_ref()
                .and_then(|container| match container {
                    ManifestWorkspaceContainerRef::Named(name) => Some(name.as_str()),
                    ManifestWorkspaceContainerRef::Inline(_) => None,
                }),
            Some("stack")
        );
        assert_eq!(system.user.as_deref(), Some("dev"));
        assert_eq!(system.home.as_deref(), Some("/home/dev"));
        let workspace = system.workspaces.get("app").expect("app workspace");
        assert_eq!(
            workspace.working_dir.as_deref(),
            Some("/workspace-root/app")
        );
        assert_eq!(
            workspace.mounts,
            vec![
                "../underlay".to_owned(),
                "../poodle:/workspace-root/poodle".to_owned()
            ]
        );
    }

    #[test]
    fn systems_config_accepts_working_dir() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
working_dir = "/workspace-root"

[systems.dev.workspaces.app]
working_dir = "/workspace-root/app"
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(system.working_dir.as_deref(), Some("/workspace-root"));

        let workspace = system.workspaces.get("app").expect("app workspace");
        assert_eq!(
            workspace.working_dir.as_deref(),
            Some("/workspace-root/app")
        );
    }

    #[test]
    fn systems_config_accepts_isolation_adoption() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems.dev]
container = "stack"
isolation = [{ repo = "../poodle" }, { repo = "../underlay" }]
"#,
        )
        .expect("parse systems");

        let system = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(
            system
                .isolation
                .iter()
                .map(|entry| entry.repo.as_str())
                .collect::<Vec<_>>(),
            vec!["../poodle", "../underlay"]
        );
    }

    #[test]
    fn isolation_config_accepts_path_list() {
        let parsed: IsolationWrapper = toml::from_str(
            r#"
[isolation]
paths = ["node_modules", ".svelte-kit", "dist"]
"#,
        )
        .expect("parse isolation");

        assert_eq!(
            parsed.isolation.paths,
            vec![
                "node_modules".to_owned(),
                ".svelte-kit".to_owned(),
                "dist".to_owned()
            ]
        );
    }

    #[test]
    fn container_config_accepts_dns_domain_and_tls() {
        let parsed: ContainerWrapper = toml::from_str(
            r#"
[containers]
default = "web"

[containers.web]
compose_file = "infra/dev/docker-compose.yml"
primary_service = "app"

[containers.web.dns]
routes = [
  { domain = "clientname.test", tls = true, port = 4173, service = "app" }
]
"#,
        )
        .expect("parse containers");

        let dns = parsed
            .containers
            .environments
            .get("web")
            .expect("web container")
            .dns
            .as_ref()
            .expect("dns config");
        assert_eq!(dns.routes.len(), 1);
        assert_eq!(dns.routes[0].domain, "clientname.test");
        assert_eq!(dns.routes[0].tls, Some(true));
        assert_eq!(dns.routes[0].port, Some(4173));
        assert_eq!(dns.routes[0].service.as_deref(), Some("app"));
    }

    #[test]
    fn container_dns_config_defaults_tls_to_none() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "clientname.test" }
]
"#,
        )
        .expect("parse dns");

        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(parsed.routes[0].domain, "clientname.test");
        assert_eq!(parsed.routes[0].tls, None);
        assert_eq!(parsed.routes[0].port, None);
        assert_eq!(parsed.routes[0].service, None);
    }

    #[test]
    fn container_dns_domains_sugar_expands_with_defaults() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = [
  "dev.example",
  "admin.example",
  "dr.example",
]
domain_defaults = { tls = true, service = "tunnel" }
"#,
        )
        .expect("parse dns sugar");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 3);
        let domains: Vec<&str> = resolved.iter().map(|r| r.domain.as_str()).collect();
        assert_eq!(domains, ["dev.example", "admin.example", "dr.example"]);
        for route in &resolved {
            assert_eq!(route.tls, Some(true));
            assert_eq!(route.service.as_deref(), Some("tunnel"));
            assert_eq!(route.port, None);
        }
    }

    #[test]
    fn container_dns_literal_route_overrides_sugar_for_same_domain() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "dev.example", port = 9000, service = "custom" },
]
domains = ["dev.example", "admin.example"]
domain_defaults = { tls = true, service = "tunnel" }
"#,
        )
        .expect("parse dns sugar with literal");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 2);
        // Literal entries come first.
        assert_eq!(resolved[0].domain, "dev.example");
        assert_eq!(resolved[0].port, Some(9000));
        assert_eq!(resolved[0].service.as_deref(), Some("custom"));
        // Literal route's tls is None — not overridden by sugar default.
        assert_eq!(resolved[0].tls, None);
        // Sugar entry for the uncovered domain still expands.
        assert_eq!(resolved[1].domain, "admin.example");
        assert_eq!(resolved[1].tls, Some(true));
        assert_eq!(resolved[1].service.as_deref(), Some("tunnel"));
    }

    #[test]
    fn container_dns_target_host_propagates_from_defaults_to_sugar_routes() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["dev.example", "admin.example"]
domain_defaults = { tls = true, target_host = "127.0.0.1:8080" }
"#,
        )
        .expect("parse dns sugar with target_host");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 2);
        for route in &resolved {
            assert_eq!(route.tls, Some(true));
            assert_eq!(route.target_host.as_deref(), Some("127.0.0.1:8080"));
            assert_eq!(route.service, None);
        }
    }

    #[test]
    fn container_dns_target_host_on_literal_route_round_trips() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "dev.example", tls = true, target_host = "127.0.0.1:8080" },
]
"#,
        )
        .expect("parse literal route with target_host");

        assert_eq!(parsed.routes.len(), 1);
        assert_eq!(
            parsed.routes[0].target_host.as_deref(),
            Some("127.0.0.1:8080")
        );
        assert_eq!(parsed.routes[0].service, None);
    }

    #[test]
    fn container_dns_domains_without_defaults_yields_unset_route_fields() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["a.example"]
"#,
        )
        .expect("parse domains-only");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].domain, "a.example");
        assert_eq!(resolved[0].tls, None);
        assert_eq!(resolved[0].port, None);
        assert_eq!(resolved[0].service, None);
    }

    #[test]
    fn container_dns_resolved_routes_skips_blank_domain_entries() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
domains = ["", "  ", "real.example"]
"#,
        )
        .expect("parse with blanks");

        let resolved = parsed.resolved_routes();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].domain, "real.example");
    }

    #[test]
    fn container_dns_config_accepts_additional_routes() {
        let parsed: ManifestContainerDnsConfig = toml::from_str(
            r#"
routes = [
  { domain = "clientname.test" },
  { domain = "admin.clientname.test", port = 8081, service = "admin" },
  { domain = "mailpit.clientname.test", port = 8025, tls = true, service = "mailpit" }
]
"#,
        )
        .expect("parse dns with routes");

        assert_eq!(parsed.routes.len(), 3);
        assert_eq!(parsed.routes[0].domain, "clientname.test");
        assert_eq!(parsed.routes[0].port, None);
        assert_eq!(parsed.routes[0].service, None);
        assert_eq!(parsed.routes[0].tls, None);
        assert_eq!(parsed.routes[1].domain, "admin.clientname.test");
        assert_eq!(parsed.routes[1].port, Some(8081));
        assert_eq!(parsed.routes[1].service.as_deref(), Some("admin"));
        assert_eq!(parsed.routes[1].tls, None);
        assert_eq!(parsed.routes[2].domain, "mailpit.clientname.test");
        assert_eq!(parsed.routes[2].port, Some(8025));
        assert_eq!(parsed.routes[2].service.as_deref(), Some("mailpit"));
        assert_eq!(parsed.routes[2].tls, Some(true));
    }

    #[test]
    fn systems_config_accepts_named_workspaces() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"
container = "app"
user = "dev"
home = "/home/dev"

[systems.dev.workspaces.app]
working_dir = "/workspace"
"#,
        )
        .expect("parse systems");

        assert_eq!(parsed.systems.default.as_deref(), Some("dev"));
        let dev = parsed.systems.systems.get("dev").expect("dev system");
        assert_eq!(dev.default_workspace.as_deref(), Some("app"));
        let app = dev.workspaces.get("app").expect("app workspace");
        assert_eq!(app.working_dir.as_deref(), Some("/workspace"));
        assert_eq!(app.user, None);
        match dev.container.as_ref().expect("workspace default container") {
            ManifestWorkspaceContainerRef::Named(name) => assert_eq!(name, "app"),
            ManifestWorkspaceContainerRef::Inline(_) => {
                panic!("expected named workspace container reference")
            }
        }
    }

    #[test]
    fn systems_config_accepts_inline_workspace_container_shortcut() {
        let parsed: SystemWrapper = toml::from_str(
            r#"
[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = { image = "node:22", mount = "./:/workspace", shell = "bash" }
"#,
        )
        .expect("parse systems");

        let app = parsed
            .systems
            .systems
            .get("dev")
            .expect("dev system")
            .workspaces
            .get("app")
            .expect("app workspace");
        match app.container.as_ref().expect("workspace container") {
            ManifestWorkspaceContainerRef::Inline(ManifestInlineWorkspaceContainerConfig {
                image,
                mount,
                extra,
            }) => {
                assert_eq!(image.as_deref(), Some("node:22"));
                assert_eq!(mount.as_deref(), Some("./:/workspace"));
                assert_eq!(
                    extra.get("shell"),
                    Some(&toml::Value::String("bash".to_owned()))
                );
            }
            ManifestWorkspaceContainerRef::Named(_) => {
                panic!("expected inline workspace container shortcut")
            }
        }
    }

    #[test]
    fn container_host_mounts_accept_legacy_string_form() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = ["./:/workspace", "./assets:/srv/assets:ro"]
"#,
        )
        .expect("parse legacy string mounts");

        assert_eq!(parsed.mounts.len(), 2);
        match &parsed.mounts[0] {
            ManifestContainerHostMount::Spec(value) => assert_eq!(value, "./:/workspace"),
            ManifestContainerHostMount::Table(_) => panic!("expected spec form"),
        }
        match &parsed.mounts[1] {
            ManifestContainerHostMount::Spec(value) => assert_eq!(value, "./assets:/srv/assets:ro"),
            ManifestContainerHostMount::Table(_) => panic!("expected spec form"),
        }
    }

    #[test]
    fn container_host_mounts_accept_structured_external_form() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  { host = "${PERSONAL_SSH_CONFIG}",
    container = "/home/dev/.ssh/config",
    external = true,
    options = ["ro"] },
]
"#,
        )
        .expect("parse structured mount");

        assert_eq!(parsed.mounts.len(), 1);
        match &parsed.mounts[0] {
            ManifestContainerHostMount::Table(table) => {
                assert_eq!(table.host, "${PERSONAL_SSH_CONFIG}");
                assert_eq!(table.container, "/home/dev/.ssh/config");
                assert!(table.external);
                assert_eq!(table.options, vec!["ro".to_owned()]);
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mounts_mix_legacy_and_structured_in_same_array() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  "./:/workspace",
  { host = "~/.config/effigy", container = "/etc/effigy", external = true },
]
"#,
        )
        .expect("parse mixed mounts");

        assert_eq!(parsed.mounts.len(), 2);
        assert!(matches!(
            parsed.mounts[0],
            ManifestContainerHostMount::Spec(_)
        ));
        match &parsed.mounts[1] {
            ManifestContainerHostMount::Table(table) => {
                assert_eq!(table.host, "~/.config/effigy");
                assert!(table.external);
                assert!(table.options.is_empty());
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mount_table_defaults_external_to_false() {
        let parsed: ManifestContainerHostConfig = toml::from_str(
            r#"
mounts = [
  { host = "./assets", container = "/srv/assets" },
]
"#,
        )
        .expect("parse defaults");

        match &parsed.mounts[0] {
            ManifestContainerHostMount::Table(table) => {
                assert!(!table.external);
                assert!(table.options.is_empty());
            }
            ManifestContainerHostMount::Spec(_) => panic!("expected table form"),
        }
    }

    #[test]
    fn container_host_mount_table_rejects_unknown_fields() {
        let result: Result<ManifestContainerHostConfig, _> = toml::from_str(
            r#"
mounts = [
  { host = "./", container = "/workspace", bogus = "value" },
]
"#,
        );
        assert!(result.is_err(), "expected unknown-field rejection");
    }
}
