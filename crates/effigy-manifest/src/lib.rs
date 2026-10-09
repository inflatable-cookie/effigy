//! Effigy manifest loading, composition, and bundle source resolution.
//!
//! This crate turns `effigy.toml` (and included fragments) into [`LoadedCatalog`]
//! values the runner uses for task routing, container wiring, and deploy/state
//! surfaces. User-facing manifest patterns live in `docs/guides/022-manifest-cookbook.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod bundles;
mod composition;
pub mod config_sections;
mod draft_defs;
pub mod execution_binding;
mod loaded_catalog;
mod manifest_section;
pub mod portfolio;
mod qa_groups;
mod task_defs;
pub mod task_runtime;
mod test_config;
pub mod user_config;

pub use effigy_core::repo_markers::TASK_MANIFEST_FILE;
pub use portfolio::{load_portfolio, Portfolio, PORTFOLIO_FILE};

pub use bundles::{
    inspect_bundle_source, sync_bundle_source, BundleInputSpec, BundleInputType,
    BundleSourceInspectReport, BundleSourceType, BundleSpec, BundleSyncReport,
};
pub use composition::{
    load_docs_policy_graph_config, load_task_manifest_with_inspection, LoadedTaskManifest,
    ManifestCompositionEdge, ManifestCompositionOverride, ManifestCompositionValueSource,
};
pub use config_sections::{
    load_committed_docs_policy_sources, ManifestBootstrapConfig, ManifestBootstrapRun,
    ManifestBootstrapStart, ManifestBootstrapStartEntry, ManifestBootstrapStartTable,
    ManifestBootstrapSubmodulesPolicy, ManifestBundleBase, ManifestBundleConfig,
    ManifestContainerConfig, ManifestContainerDataConfig, ManifestContainerDnsConfig,
    ManifestContainerDnsDomainDefaults, ManifestContainerDnsRouteConfig, ManifestContainerDriver,
    ManifestContainerExecAliasConfig, ManifestContainerExecAliasTableConfig,
    ManifestContainerHostConfig, ManifestContainerHostMount, ManifestContainerHostMountTable,
    ManifestContainerHostProcess, ManifestContainerHostProcessRestart, ManifestContainerOnTaskExit,
    ManifestContainerSecretDelivery, ManifestContainerSecretsConfig,
    ManifestContainerServiceConfig, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestDataConfig, ManifestDataTargetConfig, ManifestDemoConfig,
    ManifestDemoMode, ManifestDemoStatus, ManifestDistributionConfig,
    ManifestDistributionMetadataConfig, ManifestDistributionPackageConfig,
    ManifestDistributionPreflightConfig, ManifestDocsPolicyConfig,
    ManifestDocsPolicyGraphCardinality, ManifestDocsPolicyGraphConfig,
    ManifestDocsPolicyGraphCurrentnessClass, ManifestDocsPolicyGraphCurrentnessConfig,
    ManifestDocsPolicyGraphFieldConfig, ManifestDocsPolicyGraphKindConfig,
    ManifestDocsPolicyGraphRelationConfig, ManifestDocsPolicySourcesConfig,
    ManifestEnvSchemaConfig, ManifestInlineWorkspaceContainerConfig, ManifestIsolationAdoption,
    ManifestIsolationConfig, ManifestJsPackageManager, ManifestPackageManagerConfig,
    ManifestReleaseConfig, ManifestScanConfig, ManifestSecretKeyConfig, ManifestSecretTarget,
    ManifestSecretsBackend, ManifestSecretsConfig, ManifestSecretsExternalConfig,
    ManifestSecretsUnlockPolicy, ManifestSecretsVaultConfig, ManifestSecretsVaultIdentity,
    ManifestShellConfig, ManifestSystemConfig, ManifestSystemMount, ManifestSystemMountTable,
    ManifestSystemsConfig, ManifestTaskDefaultsConfig, ManifestWorkspaceConfig,
    ManifestWorkspaceContainerRef,
};
use draft_defs::deserialize_drafts;
pub use draft_defs::{
    deserialize_drafts as deserialize_draft_definitions, draft_source_map, ManifestDraft,
    ManifestDraftDate, ManifestDraftLikeDefinition, ManifestDraftTable,
};
pub use execution_binding::{
    resolve_task_execution_binding, resolve_task_execution_binding_from_parts,
    resolve_task_execution_binding_from_systems, ExecutionBindingResolveError,
    ResolvedInlineWorkspaceContainer, ResolvedTaskExecutionBinding, ResolvedWorkspaceBinding,
    ResolvedWorkspaceContainer,
};
pub use loaded_catalog::{
    env_schema_declaring_catalog, DeferredCommand, LoadedCatalog, TaskResolverFn, TaskSelection,
};
pub use qa_groups::{
    path_pattern_matches, validate_qa_name_grammar, ManifestQaCoverageGap, ManifestQaGroup,
    ManifestQaGroupMember, ManifestQaGroupTable, ManifestQaLifecycle, ManifestQaMemberKind,
    ManifestQaMemberSurface, ManifestQaScopePolicy, ManifestQaSection, QaGroupDefinitionContext,
    QaScopeToken,
};
use task_defs::deserialize_tasks;
pub use task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestInlineTaskDefinition,
    ManifestManagedConcurrentEntry, ManifestManagedProfile, ManifestManagedRun,
    ManifestManagedRunStep, ManifestManagedRunStepTable, ManifestRunStepEnv, ManifestTask,
    ManifestTaskAdmission, ManifestTaskCache, ManifestTaskLikeDefinition,
    ManifestTaskOrReferenceDefinition, ManifestTaskRunIn, ManifestTaskSecretsMode,
};
use test_config::ManifestTestConfig;
pub use test_config::{
    ManifestCargoEnvMatchMode, ManifestTestSuite, ManifestTestSuiteTeardownPolicy,
};
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
    /// Lifecycle-labelled provisional definitions. Excluded from every
    /// published discovery surface; reachable only through `effigy draft`.
    #[serde(default, deserialize_with = "deserialize_drafts")]
    pub drafts: BTreeMap<String, ManifestDraft>,
    /// Maintained bounded QA groups keyed under `[qa.groups.<name>]`
    /// (contract 051). Selected only through the explicit `tasks qa-group`
    /// surfaces; never task aliases and never implicit discovery.
    #[serde(default)]
    pub qa: Option<ManifestQaSection>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCatalog {
    pub alias: Option<String>,
    #[serde(default)]
    pub members: BTreeMap<String, String>,
    #[serde(default)]
    pub graph: Option<ManifestCatalogGraph>,
}

/// `[catalog.graph]` posture for one catalog.
///
/// The table answers how the parent workspace indexes this catalog. It never
/// contributes membership, renames a catalog, or grants recursive discovery.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestCatalogGraph {
    #[serde(default)]
    pub segmented: bool,
    #[serde(default)]
    pub independent: bool,
}

/// Normalized `[catalog.graph]` posture used by graph consumers.
///
/// Both fields default to `false`. `independent` only has meaning together
/// with `segmented`; a composed manifest that sets `independent = true`
/// without `segmented = true` is rejected during manifest validation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CatalogGraphPosture {
    pub segmented: bool,
    pub independent: bool,
}

impl CatalogGraphPosture {
    /// The folded posture: indexed in the parent corpus, parent storage.
    pub fn folded() -> Self {
        Self::default()
    }

    pub fn is_folded(&self) -> bool {
        !self.segmented
    }
}

impl ManifestCatalog {
    /// Normalized graph posture declared by this catalog manifest.
    pub fn graph_posture(&self) -> CatalogGraphPosture {
        match self.graph.as_ref() {
            Some(graph) => CatalogGraphPosture {
                segmented: graph.segmented,
                independent: graph.independent,
            },
            None => CatalogGraphPosture::folded(),
        }
    }
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
        if self.tasks.contains_key("test") {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: "`tasks.test` was removed in v0.11 because `effigy test` is always the built-in orchestrator; move the command to a named `[test.suites]` entry"
                    .to_owned(),
            });
        }
        if let Some(catalog) = self.catalog.as_ref() {
            for (member, directory) in &catalog.members {
                if member.trim().is_empty() || directory.trim().is_empty() {
                    return Err(ManifestError::Compose {
                        path: manifest_path.to_path_buf(),
                        detail: "catalog member handles and directories must be non-empty strings"
                            .to_owned(),
                    });
                }
            }
            if let Some(graph) = catalog.graph.as_ref() {
                if graph.independent && !graph.segmented {
                    return Err(ManifestError::Compose {
                        path: manifest_path.to_path_buf(),
                        detail: "[catalog.graph] `independent = true` requires `segmented = true`; a folded catalog is always indexed in its parent graph corpus"
                            .to_owned(),
                    });
                }
            }
        }
        for (demo_id, demo) in &self.demos {
            demo.validate(manifest_path, demo_id)?;
        }
        for (draft_name, draft) in &self.drafts {
            if self.tasks.contains_key(draft_name) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "`{draft_name}` is declared in both `[tasks]` and `[drafts]`; a published task and a draft cannot share a name in the same effective catalog"
                    ),
                });
            }
            let _ = draft;
        }
        for (task_name, task) in &self.tasks {
            if let Some(reference) = first_draft_step_reference(task) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "published task `{task_name}` references draft `{reference}`; published tasks cannot depend on disposable drafts (move the step into a draft or publish the target task)"
                    ),
                });
            }
        }
        if let Some(docs_policy) = self.docs_policy.as_ref() {
            docs_policy.validate(manifest_path)?;
        }
        if let Some(containers) = self.containers.as_ref() {
            for (container_name, container) in &containers.environments {
                if let Some(dns) = container.dns.as_ref() {
                    validate_dns_routes(manifest_path, container_name, dns)?;
                }
                validate_host_processes(
                    manifest_path,
                    container_name,
                    &container.host_processes,
                    container.dns.as_ref(),
                )?;
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
    dns: Option<&crate::config_sections::ManifestContainerDnsConfig>,
) -> Result<(), ManifestError> {
    use std::collections::{HashMap, HashSet};

    let mut seen = HashSet::<String>::new();
    let mut listener_domains = HashSet::<String>::new();
    let mut dependency_env_names = HashSet::<String>::new();
    let mut entries_by_name = HashMap::new();
    for (index, entry) in entries.iter().enumerate() {
        let scope = format!("containers.{container_name}.host_processes[{index}]");
        let trimmed_name = entry.name.trim();
        if trimmed_name.is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{scope}.name is empty"),
            });
        }
        if trimmed_name != entry.name {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{scope}.name cannot contain leading or trailing whitespace"),
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
        let dependency_env_name = trimmed_name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character.to_ascii_uppercase()
                } else {
                    '_'
                }
            })
            .collect::<String>();
        if !dependency_env_names.insert(dependency_env_name) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "{scope}.name collides with another host process after dependency environment normalization"
                ),
            });
        }
        entries_by_name.insert(trimmed_name.to_owned(), entry);
        if entry.run.trim().is_empty() {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!("{scope}.run is empty"),
            });
        }
        if let Some(cwd) = entry.cwd.as_deref() {
            let path = std::path::Path::new(cwd);
            if path.is_absolute()
                || path
                    .components()
                    .any(|component| component == std::path::Component::ParentDir)
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("{scope}.cwd must be a relative path inside the checkout"),
                });
            }
        }
        for key in entry.env.keys() {
            if key.is_empty()
                || !key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                || key.starts_with("EFFIGY_MANAGED_HOST_")
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.env contains invalid or reserved environment key `{key}`"
                    ),
                });
            }
        }
        if let Some(listener) = entry.listener.as_ref() {
            let bind = listener.bind.parse::<std::net::SocketAddr>().map_err(|_| {
                ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("{scope}.listener.bind must be a loopback socket address"),
                }
            })?;
            if !bind.ip().is_loopback() || (bind.port() > 0 && bind.port() < 1024) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.listener.bind must use an unprivileged loopback address"
                    ),
                });
            }
            let readiness = &listener.readiness;
            if !readiness.path.starts_with('/')
                || readiness.path.chars().any(|character| {
                    character.is_ascii_control() || character.is_ascii_whitespace()
                })
                || readiness.path.contains('#')
                || !(100..=599).contains(&readiness.status)
                || readiness.timeout_secs == 0
                || readiness.timeout_secs > 3_600
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.listener.readiness requires an absolute HTTP path, status 100–599, and timeout_secs from 1 through 3600"
                    ),
                });
            }
            if listener.route.domain.trim() != listener.route.domain
                || listener.route.domain.ends_with('.')
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.listener.route.domain must be canonical without surrounding whitespace or a trailing dot"
                    ),
                });
            }
            validate_dns_domain_name(
                manifest_path,
                &listener.route.domain,
                &format!("{scope}.listener.route.domain"),
            )?;
            if !listener_domains.insert(listener.route.domain.to_ascii_lowercase()) {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "containers.{container_name}.host_processes contains duplicate managed listener route `{}`",
                        listener.route.domain
                    ),
                });
            }
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

    if let Some(dns) = dns {
        let static_domains = dns
            .resolved_routes()
            .into_iter()
            .map(|route| route.domain.to_ascii_lowercase())
            .collect::<HashSet<_>>();
        if let Some(domain) = listener_domains
            .iter()
            .find(|domain| static_domains.contains(*domain))
        {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.host_processes listener route `{domain}` conflicts with a static containers.{container_name}.dns route"
                ),
            });
        }
    }

    for (index, entry) in entries.iter().enumerate() {
        let scope = format!("containers.{container_name}.host_processes[{index}]");
        let mut dependencies = HashSet::new();
        for raw_dependency in &entry.depends_on {
            let dependency = raw_dependency.trim();
            if dependency.is_empty()
                || dependency != raw_dependency
                || dependency == entry.name.trim()
                || !dependencies.insert(dependency.to_owned())
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("{scope}.depends_on must contain unique other process names"),
                });
            }
            let Some(target) = entries_by_name.get(dependency) else {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.depends_on references unknown host process `{dependency}`"
                    ),
                });
            };
            if target.listener.is_none() {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!(
                        "{scope}.depends_on references `{dependency}`, which has no managed listener"
                    ),
                });
            }
        }
    }

    let mut complete = HashSet::new();
    let mut visiting = HashSet::new();
    fn visit(
        name: &str,
        entries: &HashMap<String, &crate::config_sections::ManifestContainerHostProcess>,
        visiting: &mut HashSet<String>,
        complete: &mut HashSet<String>,
    ) -> bool {
        if complete.contains(name) {
            return true;
        }
        if !visiting.insert(name.to_owned()) {
            return false;
        }
        let Some(entry) = entries.get(name) else {
            return false;
        };
        for dependency in &entry.depends_on {
            if !visit(dependency, entries, visiting, complete) {
                return false;
            }
        }
        visiting.remove(name);
        complete.insert(name.to_owned());
        true
    }
    for name in entries_by_name.keys() {
        if !visit(name, &entries_by_name, &mut visiting, &mut complete) {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.host_processes contains a managed listener dependency cycle"
                ),
            });
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

/// First explicit `{ draft = "..." }` step reference in a published task body,
/// if any. Published tasks must never depend on disposable drafts.
fn first_draft_step_reference(task: &ManifestTask) -> Option<String> {
    fn step_draft(step: &ManifestManagedRunStep) -> Option<String> {
        match step {
            ManifestManagedRunStep::Command(_) => None,
            ManifestManagedRunStep::Step(table) => table.draft.clone(),
        }
    }

    let mut references = Vec::new();
    if let Some(ManifestManagedRun::Sequence(steps)) = task.run.as_ref() {
        references.extend(steps.iter().filter_map(step_draft));
    }
    for entry in &task.concurrent {
        references.extend(entry.setup.iter().filter_map(step_draft));
    }
    for profile in task.profiles.values() {
        for entry in &profile.concurrent {
            references.extend(entry.setup.iter().filter_map(step_draft));
        }
    }
    references.into_iter().next()
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

    #[test]
    fn managed_host_listener_dynamic_bind_and_readiness_pass() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "frontend"
run = "exec ./start-frontend"

[containers.web.host_processes.listener]
bind = "127.0.0.1:0"

[containers.web.host_processes.listener.readiness]
path = "/health"
status = 204
timeout_secs = 30

[containers.web.host_processes.listener.route]
domain = "frontend.example.test"
tls = true

[[containers.web.host_processes]]
name = "consumer"
run = "exec ./consume"
depends_on = ["frontend"]
"#,
        );
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("valid listener and dependency declarations");
    }

    #[test]
    fn managed_host_listener_rejects_non_loopback_bind_and_bad_path() {
        let non_loopback = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "frontend"
run = "exec ./start-frontend"

[containers.web.host_processes.listener]
bind = "0.0.0.0:4173"

[containers.web.host_processes.listener.readiness]
path = "/health"

[containers.web.host_processes.listener.route]
domain = "frontend.example.test"
"#,
        );
        assert!(err(&non_loopback).contains("unprivileged loopback address"));

        let bad_path = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "frontend"
run = "exec ./start-frontend"

[containers.web.host_processes.listener]
bind = "127.0.0.1:4173"

[containers.web.host_processes.listener.readiness]
path = "/health status=200"

[containers.web.host_processes.listener.route]
domain = "frontend.example.test"
"#,
        );
        assert!(err(&bad_path).contains("absolute HTTP path"));

        let unbounded_timeout = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "frontend"
run = "exec ./start-frontend"

[containers.web.host_processes.listener]
bind = "127.0.0.1:4173"

[containers.web.host_processes.listener.readiness]
path = "/health"
timeout_secs = 3601

[containers.web.host_processes.listener.route]
domain = "frontend.example.test"
"#,
        );
        assert!(err(&unbounded_timeout).contains("timeout_secs from 1 through 3600"));
    }

    #[test]
    fn managed_host_listener_dependency_cycles_are_rejected() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[[containers.web.host_processes]]
name = "one"
run = "exec ./one"
depends_on = ["two"]

[containers.web.host_processes.listener]
bind = "127.0.0.1:0"
readiness = { path = "/health" }
route = { domain = "one.example.test" }

[[containers.web.host_processes]]
name = "two"
run = "exec ./two"
depends_on = ["one"]

[containers.web.host_processes.listener]
bind = "127.0.0.1:0"
readiness = { path = "/health" }
route = { domain = "two.example.test" }
"#,
        );
        assert!(err(&manifest).contains("dependency cycle"));
    }
}
