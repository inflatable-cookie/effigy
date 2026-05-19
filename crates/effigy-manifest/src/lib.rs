use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod bundles;
mod composition;
pub mod config_sections;
pub mod execution_binding;
mod loaded_catalog;
mod manifest_section;
mod task_defs;
pub mod task_runtime;
mod test_config;
pub mod user_config;

/// Filename of the per-catalog Effigy manifest file (`effigy.toml`).
///
/// Canonical copy lives here. A handful of historical call sites in
/// `effigy-bootstrap`, `effigy-release`, and `effigy-routing` still
/// inline their own copies to avoid an extra cross-crate dep; they
/// should migrate to this constant when the opportunity arises.
pub const TASK_MANIFEST_FILE: &str = "effigy.toml";

pub use bundles::{
    inspect_bundle_source, sync_bundle_source, BundleInputSpec, BundleInputType,
    BundleSourceInspectReport, BundleSourceType, BundleSpec, BundleSyncReport,
};
pub use composition::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestCompositionEdge,
    ManifestCompositionOverride, ManifestCompositionValueSource,
};
pub use config_sections::{
    ManifestBootstrapConfig, ManifestBootstrapRun, ManifestBootstrapStart,
    ManifestBootstrapStartEntry, ManifestBootstrapStartTable, ManifestBootstrapSubmodulesPolicy,
    ManifestBundleBase, ManifestBundleConfig, ManifestContainerConfig, ManifestContainerDataConfig,
    ManifestContainerDnsConfig, ManifestContainerDnsDomainDefaults,
    ManifestContainerDnsRouteConfig, ManifestContainerDriver, ManifestContainerExecAliasConfig,
    ManifestContainerExecAliasTableConfig, ManifestContainerHostConfig, ManifestContainerHostMount,
    ManifestContainerHostMountTable, ManifestContainerHostProcess,
    ManifestContainerHostProcessRestart, ManifestContainerOnTaskExit,
    ManifestContainerServiceConfig, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestDataConfig, ManifestDataTargetConfig, ManifestDemoConfig,
    ManifestDemoMode, ManifestDemoStatus, ManifestDistributionConfig,
    ManifestDistributionMetadataConfig, ManifestDistributionPackageConfig,
    ManifestDistributionPreflightConfig, ManifestDocsPolicyConfig, ManifestEnvSchemaConfig,
    ManifestInlineWorkspaceContainerConfig, ManifestIsolationAdoption, ManifestIsolationConfig,
    ManifestJsPackageManager, ManifestPackageManagerConfig, ManifestReleaseConfig,
    ManifestScanConfig, ManifestSecretKeyConfig, ManifestSecretTarget, ManifestSecretsBackend,
    ManifestSecretsConfig, ManifestSecretsExternalConfig, ManifestSecretsUnlockPolicy,
    ManifestSecretsVaultConfig, ManifestSecretsVaultIdentity, ManifestShellConfig,
    ManifestSystemConfig, ManifestSystemsConfig, ManifestTaskDefaultsConfig,
    ManifestWorkspaceConfig, ManifestWorkspaceContainerRef,
};
pub use execution_binding::{
    resolve_task_execution_binding, resolve_task_execution_binding_from_parts,
    resolve_task_execution_binding_from_systems, ExecutionBindingResolveError,
    ResolvedInlineWorkspaceContainer, ResolvedTaskExecutionBinding, ResolvedWorkspaceBinding,
    ResolvedWorkspaceContainer,
};
pub use loaded_catalog::{DeferredCommand, LoadedCatalog, TaskResolverFn, TaskSelection};
use task_defs::deserialize_tasks;
pub use task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestInlineTaskDefinition,
    ManifestManagedConcurrentEntry, ManifestManagedProfile, ManifestManagedRun,
    ManifestManagedRunStep, ManifestManagedRunStepTable, ManifestRunStepEnv, ManifestTask,
    ManifestTaskCache, ManifestTaskLikeDefinition, ManifestTaskOrReferenceDefinition,
    ManifestTaskRunIn, ManifestTaskSecretsMode,
};
use test_config::ManifestTestConfig;
pub use test_config::{ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};
pub use user_config::{
    load_user_config, load_user_config_from, save_user_config, save_user_config_to,
    user_config_path, with_test_user_config_home, LibraryMount, UserBundleConfig, UserConfig,
    UserContainerBackendPreference, UserContainersConfig, USER_CONFIG_FILE,
};

#[derive(Debug)]
pub enum ManifestError {
    Read {
        path: PathBuf,
        error: std::io::Error,
    },
    Parse {
        path: PathBuf,
        error: toml::de::Error,
    },
    Compose {
        path: PathBuf,
        detail: String,
    },
    Render {
        path: PathBuf,
        detail: String,
    },
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            Self::Parse { path, error } => {
                write!(f, "failed to parse {}: {error}", path.display())
            }
            Self::Compose { path, detail } => {
                write!(f, "manifest compose failed in {}: {detail}", path.display())
            }
            Self::Render { path, detail } => {
                write!(f, "failed to render {}: {detail}", path.display())
            }
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskManifest {
    #[serde(default)]
    pub catalog: Option<ManifestCatalog>,
    #[serde(default)]
    pub bundle: Option<ManifestBundleConfig>,
    #[serde(default)]
    pub defer: Option<ManifestDefer>,
    #[serde(default)]
    pub env: BTreeMap<String, ManifestEnvEntry>,
    #[serde(default)]
    pub data: Option<ManifestDataConfig>,
    #[serde(default)]
    pub state: Option<toml::Value>,
    /// Raw deployment transaction config keyed by environment.
    ///
    /// This intentionally remains a raw TOML value in the manifest crate so
    /// the runner can evolve `[deploy.<env>]` without forcing every manifest
    /// consumer to depend on deployment transaction semantics.
    #[serde(default)]
    pub deploy: Option<toml::Value>,
    #[serde(default)]
    pub test: Option<ManifestTestConfig>,
    #[serde(default)]
    pub package_manager: Option<ManifestPackageManagerConfig>,
    #[serde(default)]
    pub scan: Option<ManifestScanConfig>,
    #[serde(default)]
    pub shell: Option<ManifestShellConfig>,
    #[serde(default)]
    pub env_schema: Option<ManifestEnvSchemaConfig>,
    #[serde(default)]
    pub secrets: Option<ManifestSecretsConfig>,
    #[serde(default)]
    pub docs_policy: Option<ManifestDocsPolicyConfig>,
    #[serde(default)]
    pub task_defaults: Option<ManifestTaskDefaultsConfig>,
    #[serde(default)]
    pub bootstrap: Option<ManifestBootstrapConfig>,
    #[serde(default)]
    pub isolation: Option<ManifestIsolationConfig>,
    #[serde(default)]
    pub containers: Option<ManifestContainersConfig>,
    #[serde(default)]
    pub systems: Option<ManifestSystemsConfig>,
    #[serde(default)]
    pub distribution: Option<ManifestDistributionConfig>,
    #[serde(default)]
    pub release: Option<ManifestReleaseConfig>,
    #[serde(default)]
    pub demos: BTreeMap<String, ManifestDemoConfig>,
    #[serde(default, deserialize_with = "deserialize_tasks")]
    pub tasks: BTreeMap<String, ManifestTask>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCatalog {
    pub alias: Option<String>,
    #[serde(default)]
    pub discovery: Option<ManifestCatalogDiscoveryConfig>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCatalogDiscoveryConfig {
    #[serde(default)]
    pub ignore: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestDefer {
    pub run: String,
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
    #[serde(default)]
    pub builtins: Vec<String>,
}

pub fn load_task_manifest(manifest_path: &Path) -> Result<TaskManifest, ManifestError> {
    Ok(load_task_manifest_with_inspection(manifest_path)?.manifest)
}

impl TaskManifest {
    pub fn task_run_in(&self, task: &ManifestTask) -> ManifestTaskRunIn {
        task.effective_run_in(
            self.task_defaults
                .as_ref()
                .and_then(|defaults| defaults.run_in),
        )
    }

    pub fn validate(&self, manifest_path: &Path) -> Result<(), ManifestError> {
        for (demo_id, demo) in &self.demos {
            demo.validate(manifest_path, demo_id)?;
        }
        if let Some(containers) = self.containers.as_ref() {
            for (container_name, container) in &containers.environments {
                if let Some(dns) = container.dns.as_ref() {
                    validate_dns_routes(manifest_path, container_name, dns)?;
                }
                validate_host_processes(manifest_path, container_name, &container.host_processes)?;
            }
        }
        Ok(())
    }
}

fn validate_dns_routes(
    manifest_path: &Path,
    container_name: &str,
    dns: &crate::config_sections::ManifestContainerDnsConfig,
) -> Result<(), ManifestError> {
    if let Some(defaults) = dns.domain_defaults.as_ref() {
        if defaults.service.is_some() && defaults.target_host.is_some() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.dns.domain_defaults declares both `service` and `target_host`; pick one"
                ),
            });
        }
        if let Some(target) = defaults.target_host.as_deref() {
            validate_target_host_format(
                manifest_path,
                target,
                &format!("containers.{container_name}.dns.domain_defaults.target_host"),
            )?;
        }
    }
    for route in dns.resolved_routes() {
        validate_dns_domain_name(
            manifest_path,
            route.domain.as_str(),
            &format!(
                "containers.{container_name}.dns.routes[{}].domain",
                route.domain
            ),
        )?;
        if route.service.is_some() && route.target_host.is_some() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.dns.routes entry for `{}` declares both `service` and `target_host`; pick one",
                    route.domain
                ),
            });
        }
        if let Some(target) = route.target_host.as_deref() {
            validate_target_host_format(
                manifest_path,
                target,
                &format!(
                    "containers.{container_name}.dns.routes[{}].target_host",
                    route.domain
                ),
            )?;
        }
    }
    Ok(())
}

fn validate_dns_domain_name(
    manifest_path: &Path,
    raw: &str,
    field: &str,
) -> Result<(), ManifestError> {
    let trimmed = raw.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("{field} is empty"),
        });
    }
    if trimmed.len() > 253 {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("{field} = `{raw}` exceeds 253 characters"),
        });
    }
    for label in trimmed.split('.') {
        if label.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{field} = `{raw}` contains an empty label"),
            });
        }
        if label.len() > 63 {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "{field} = `{raw}` contains label `{label}` longer than 63 characters"
                ),
            });
        }
        if label.starts_with('-') || label.ends_with('-') {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "{field} = `{raw}` contains label `{label}` that starts or ends with `-`"
                ),
            });
        }
        if !label
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "{field} = `{raw}` contains label `{label}` with characters outside ASCII letters, digits, or `-`"
                ),
            });
        }
    }
    Ok(())
}

fn validate_host_processes(
    manifest_path: &Path,
    container_name: &str,
    entries: &[crate::config_sections::ManifestContainerHostProcess],
) -> Result<(), ManifestError> {
    use std::collections::HashSet;

    let mut seen = HashSet::<String>::new();
    for (index, entry) in entries.iter().enumerate() {
        let scope = format!("containers.{container_name}.host_processes[{index}]");
        let trimmed_name = entry.name.trim();
        if trimmed_name.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{scope}.name is empty"),
            });
        }
        if !trimmed_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "{scope}.name = `{trimmed_name}` must contain only ASCII letters, digits, `-`, or `_`"
                ),
            });
        }
        if !seen.insert(trimmed_name.to_owned()) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.host_processes contains duplicate name `{trimmed_name}`"
                ),
            });
        }
        if entry.run.trim().is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{scope}.run is empty"),
            });
        }
        if let Some(signal) = entry.shutdown_signal.as_deref() {
            let upper = signal.trim().to_ascii_uppercase();
            const ALLOWED: &[&str] = &["SIGTERM", "SIGINT", "SIGHUP", "SIGKILL"];
            if !ALLOWED.contains(&upper.as_str()) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.shutdown_signal = `{signal}` must be one of: SIGTERM, SIGINT, SIGHUP, SIGKILL"
                    ),
                });
            }
        }
    }
    Ok(())
}

fn validate_target_host_format(
    manifest_path: &Path,
    raw: &str,
    field: &str,
) -> Result<(), ManifestError> {
    let trimmed = raw.trim();
    let Some((host, port)) = trimmed.rsplit_once(':') else {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!(
                "{field} = `{raw}` must be in `host:port` form (e.g. `127.0.0.1:8080`)"
            ),
        });
    };
    if host.trim().is_empty() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("{field} = `{raw}` is missing a host before the `:`"),
        });
    }
    if port.parse::<u16>().is_err() {
        return Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail: format!("{field} = `{raw}` has port `{port}` that does not parse as u16"),
        });
    }
    Ok(())
}

impl ManifestDefer {
    pub fn explicitly_deferred_builtins(&self) -> BTreeSet<String> {
        self.builtins
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<String>>()
    }
}

#[cfg(test)]
mod target_host_validation_tests {
    use super::*;

    fn parse(text: &str) -> TaskManifest {
        toml::from_str(text).expect("parse manifest")
    }

    fn err(manifest: &TaskManifest) -> String {
        match manifest.validate(Path::new("/tmp/effigy.toml")) {
            Ok(_) => panic!("expected validation error"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn defaults_with_service_and_target_host_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
domains = ["a.test"]
domain_defaults = { service = "tunnel", target_host = "127.0.0.1:8080" }
"#,
        );
        let detail = err(&manifest);
        assert!(
            detail.contains("declares both `service` and `target_host`"),
            "got: {detail}"
        );
    }

    #[test]
    fn route_with_service_and_target_host_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "a.test", service = "tunnel", target_host = "127.0.0.1:8080" }]
"#,
        );
        let detail = err(&manifest);
        assert!(
            detail.contains("declares both `service` and `target_host`"),
            "got: {detail}"
        );
    }

    #[test]
    fn target_host_must_be_host_colon_port() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "a.test", target_host = "127.0.0.1" }]
"#,
        );
        let detail = err(&manifest);
        assert!(
            detail.contains("must be in `host:port` form"),
            "got: {detail}"
        );
    }

    #[test]
    fn target_host_port_must_be_u16() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "a.test", target_host = "127.0.0.1:99999" }]
"#,
        );
        let detail = err(&manifest);
        assert!(detail.contains("does not parse as u16"), "got: {detail}");
    }

    #[test]
    fn valid_target_host_passes() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
domains = ["a.test"]
domain_defaults = { tls = true, target_host = "127.0.0.1:8080" }
"#,
        );
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("expected valid manifest");
    }

    #[test]
    fn route_domain_rejects_path_characters() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
routes = [{ domain = "../escape", target_host = "127.0.0.1:8080" }]
"#,
        );
        let detail = err(&manifest);
        assert!(detail.contains("contains an empty label"), "got: {detail}");
    }

    #[test]
    fn sugar_domain_rejects_leading_dash_label() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
domains = ["-bad.example.test"]
domain_defaults = { tls = true, target_host = "127.0.0.1:8080" }
"#,
        );
        let detail = err(&manifest);
        assert!(detail.contains("starts or ends with `-`"), "got: {detail}");
    }
}

#[cfg(test)]
mod host_process_validation_tests {
    use super::*;

    fn parse(text: &str) -> TaskManifest {
        toml::from_str(text).expect("parse manifest")
    }

    fn err(manifest: &TaskManifest) -> String {
        match manifest.validate(Path::new("/tmp/effigy.toml")) {
            Ok(_) => panic!("expected validation error"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn host_process_with_empty_name_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = ""
run = "echo ok"
"#,
        );
        assert!(err(&manifest).contains("name is empty"));
    }

    #[test]
    fn host_process_with_bad_name_chars_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "tunnel/bad"
run = "echo ok"
"#,
        );
        assert!(err(&manifest).contains("must contain only ASCII letters"));
    }

    #[test]
    fn host_process_with_duplicate_name_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "tunnel"
run = "echo ok"

[[containers.web.host_processes]]
name = "tunnel"
run = "echo ok"
"#,
        );
        assert!(err(&manifest).contains("duplicate name"));
    }

    #[test]
    fn host_process_with_empty_run_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "tunnel"
run = "   "
"#,
        );
        assert!(err(&manifest).contains("run is empty"));
    }

    #[test]
    fn host_process_with_unknown_signal_is_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "tunnel"
run = "echo ok"
shutdown_signal = "SIGUSR2"
"#,
        );
        assert!(err(&manifest).contains("must be one of"));
    }

    #[test]
    fn valid_host_process_passes() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "tunnel"
run = "autossh -L 0.0.0.0:8080:127.0.0.1:80 bastion"
restart = "always"
restart_delay_ms = 2500
shutdown_signal = "SIGTERM"
shutdown_grace_secs = 10
"#,
        );
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("expected valid manifest");
    }
}
