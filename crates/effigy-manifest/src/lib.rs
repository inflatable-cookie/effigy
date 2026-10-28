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
mod lifecycle_date;
pub use lifecycle_date::ManifestLifecycleDate;
pub mod execution_binding;
mod legacy_compat;
mod loaded_catalog;
mod manifest_section;
pub mod portfolio;
mod qa_groups;
mod retired_bootstrap;
mod retired_config;
mod retired_keys;
mod retired_notice;
mod retired_sections;
pub mod sources;
mod task_defs;
pub mod task_runtime;
mod test_config;
pub mod user_config;

pub use effigy_core::repo_markers::TASK_MANIFEST_FILE;
/// Longest task `description` Effigy accepts, in characters. A description is
/// one line of human context, not documentation: the guide and the task's own
/// `--help` carry the detail.
pub const TASK_DESCRIPTION_MAX_CHARS: usize = 200;
pub use portfolio::{load_portfolio, Portfolio, PORTFOLIO_FILE};

pub use bundles::{
    bundle_update_notes, catalog_source_roots, inspect_bundle_source, prepare_bundle_sources,
    prepare_missing_bundle_sources, relocate_source_paths, relocate_source_paths_through,
    resolve_catalog_source, sync_bundle_source, BundleInputSpec, BundleInputType,
    BundleSourceInspectReport, BundleSourceType, BundleSpec, BundleSyncReport, CatalogSourceRoots,
    SourcePreparation, SourceProgress, SourceProgressEvent, SourceSettlement,
};
pub use composition::{
    load_docs_policy_graph_config, load_task_manifest_with_inspection, LoadedTaskManifest,
    ManifestCompositionEdge, ManifestCompositionOverride, ManifestCompositionValueSource,
};
pub use config_sections::{
    load_committed_docs_policy_sources, ManifestBootstrapConfig, ManifestBundleBase,
    ManifestBundleConfig, ManifestCatalogSourceConfig, ManifestContainerConfig,
    ManifestContainerDataConfig, ManifestContainerDnsConfig, ManifestContainerDnsDomainDefaults,
    ManifestContainerDnsRouteConfig, ManifestContainerDriver, ManifestContainerExecAliasConfig,
    ManifestContainerExecAliasTableConfig, ManifestContainerHostConfig, ManifestContainerHostMount,
    ManifestContainerHostMountTable, ManifestContainerHostProcess,
    ManifestContainerHostProcessRestart, ManifestContainerOnTaskExit,
    ManifestContainerSecretDelivery, ManifestContainerSecretsConfig,
    ManifestContainerServiceConfig, ManifestContainerShutdownMode, ManifestContainerStartup,
    ManifestContainersConfig, ManifestContractsConfig, ManifestDataConfig,
    ManifestDataTargetConfig, ManifestDocsPolicyConfig, ManifestDocsPolicyGraphCardinality,
    ManifestDocsPolicyGraphConfig, ManifestDocsPolicyGraphCurrentnessClass,
    ManifestDocsPolicyGraphCurrentnessConfig, ManifestDocsPolicyGraphFieldConfig,
    ManifestDocsPolicyGraphKindConfig, ManifestDocsPolicyGraphRelationConfig,
    ManifestDocsPolicySourcesConfig, ManifestEnvSchemaConfig,
    ManifestInlineWorkspaceContainerConfig, ManifestIsolationAdoption, ManifestIsolationConfig,
    ManifestJsPackageManager, ManifestPackageManagerConfig, ManifestPinnedSourceConfig,
    ManifestReleaseConfig, ManifestScanConfig, ManifestSecretKeyConfig, ManifestSecretTarget,
    ManifestSecretsBackend, ManifestSecretsConfig, ManifestSecretsExternalConfig,
    ManifestSecretsVaultConfig, ManifestSourceUpdate, ManifestSystemConfig, ManifestSystemMount,
    ManifestSystemMountTable, ManifestSystemsConfig, ManifestTaskDefaultsConfig,
    ManifestWorkspaceConfig, ManifestWorkspaceContainerRef,
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
pub use retired_config::RetiredConfigDeclaration;
pub use retired_notice::{
    set_delivery as set_retired_notice_delivery, take_pending as take_retired_notices,
    RetiredNoticeDelivery,
};
pub use sources::{
    inspect_pinned_sources, prepare_missing_pinned_sources, prepare_pinned_sources,
    resolve_pinned_sources, PinnedSourceReport, PinnedSourceSpec, PinnedSourceState,
};
use task_defs::deserialize_tasks;
pub use task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestInlineTaskDefinition,
    ManifestManagedConcurrentEntry, ManifestManagedProfile, ManifestManagedRun,
    ManifestManagedRunStep, ManifestManagedRunStepTable, ManifestRunStepEnv, ManifestTask,
    ManifestTaskAdmission, ManifestTaskCache, ManifestTaskLikeDefinition,
    ManifestTaskOrReferenceDefinition, ManifestTaskRunIn, ManifestTaskSecretsMode,
    ManifestTaskVisibility,
};
use test_config::ManifestTestConfig;
pub use test_config::{
    ManifestCargoEnvMatchMode, ManifestTestSuite, ManifestTestSuiteTeardownPolicy,
};
pub use user_config::{
    load_user_config, load_user_config_from, save_user_config, save_user_config_to,
    user_config_path, with_test_user_config_home, LibraryMount, UserBundleConfig, UserConfig,
    UserContainerBackendPreference, UserContainersConfig, UserProgramConfig, USER_CONFIG_FILE,
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

/// Top-level tables an `effigy.toml` may declare: the composition-only
/// `[manifest]` section, every [`TaskManifest`] field, and the retired tables
/// that still load for one release ([`RETIRED_MANIFEST_TOP_LEVEL_KEYS`]).
/// Schema consumers such as doctor read this list instead of keeping their own
/// copy.
pub const MANIFEST_TOP_LEVEL_KEYS: &[&str] = &[
    "manifest",
    "catalog",
    "bundle",
    "defer",
    "env",
    "data",
    "state",
    "deploy",
    "test",
    "package_manager",
    "scan",
    "env_schema",
    "secrets",
    "docs_policy",
    "task_defaults",
    "bootstrap",
    "sources",
    "isolation",
    "containers",
    "systems",
    "distribution",
    "release",
    "contracts",
    "shell",
    "demos",
    "tasks",
    "qa",
];

/// Top-level tables retired in v0.15 that still load for one release. They are
/// accepted document keys but not [`TaskManifest`] fields: the manifest drops
/// them and names each in its one deprecation notice (effigy#274).
pub const RETIRED_MANIFEST_TOP_LEVEL_KEYS: &[&str] = &["shell", "demos", "distribution"];

#[derive(Debug, serde::Deserialize)]
#[serde(remote = "Self", deny_unknown_fields)]
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
    pub env_schema: Option<ManifestEnvSchemaConfig>,
    #[serde(default)]
    pub secrets: Option<ManifestSecretsConfig>,
    #[serde(default)]
    pub docs_policy: Option<ManifestDocsPolicyConfig>,
    #[serde(default)]
    pub task_defaults: Option<ManifestTaskDefaultsConfig>,
    #[serde(default)]
    pub bootstrap: Option<ManifestBootstrapConfig>,
    /// What this manifest's retired v0.15 config read or ignored. It is never
    /// part of the TOML shape: [`load_task_manifest_with_inspection`] emits its
    /// notice once per run and a removed command refuses with its refusals.
    #[serde(skip)]
    pub retired_config: RetiredConfigDeclaration,
    /// Pinned extra sources acquired beside this checkout (contract 020).
    /// Declared as `[sources.<name>]`; replaces the removed
    /// `[bootstrap].children`.
    #[serde(default, deserialize_with = "deserialize_sources")]
    pub sources: BTreeMap<String, ManifestPinnedSourceConfig>,
    #[serde(default)]
    pub isolation: Option<ManifestIsolationConfig>,
    #[serde(default)]
    pub containers: Option<ManifestContainersConfig>,
    #[serde(default)]
    pub systems: Option<ManifestSystemsConfig>,
    #[serde(default)]
    pub release: Option<ManifestReleaseConfig>,
    #[serde(default)]
    pub contracts: Option<ManifestContractsConfig>,
    #[serde(default, deserialize_with = "deserialize_tasks")]
    pub tasks: BTreeMap<String, ManifestTask>,
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
    pub sources: BTreeMap<String, ManifestCatalogSourceConfig>,
}

fn deserialize_sources<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ManifestPinnedSourceConfig>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <toml::Value as serde::Deserialize>::deserialize(deserializer)?;
    let sources =
        <BTreeMap<String, ManifestPinnedSourceConfig> as serde::Deserialize>::deserialize(value)
            .map_err(serde::de::Error::custom)?;
    for (name, source) in &sources {
        if name.trim().is_empty() || name.contains('/') || name.contains(':') {
            return Err(serde::de::Error::custom(format!(
                "pinned source `{name}` needs a plain handle: no whitespace, '/' or ':'"
            )));
        }
        let inline = source
            .rev
            .as_deref()
            .is_some_and(|rev| !rev.trim().is_empty());
        let from_file = source
            .rev_file
            .as_deref()
            .is_some_and(|file| !file.trim().is_empty());
        match (inline, from_file) {
            (true, true) => {
                return Err(serde::de::Error::custom(format!(
                    "pinned source `{name}` declares both `rev` and `rev_file`; keep exactly one so the pin is never ambiguous"
                )))
            }
            (false, false) => {
                return Err(serde::de::Error::custom(format!(
                    "pinned source `{name}` needs `rev` or `rev_file`; a source is never advanced silently (see CHANGELOG.md)"
                )))
            }
            _ => {}
        }
    }
    Ok(sources)
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

/// The profile label the default `concurrent` list runs under.
const DEFAULT_PROFILE_LABEL: &str = "default";

impl TaskManifest {
    /// Every `start_after_ms` still declared in this manifest's tasks, as
    /// `(task, profile, entry)`. The key still delays its entry for the v0.15
    /// line but no longer orders a managed session (effigy#322).
    pub fn retired_start_after_entries(&self) -> Vec<(String, String, String)> {
        let mut retired = Vec::new();
        for (task_name, task) in &self.tasks {
            let profiles =
                std::iter::once((DEFAULT_PROFILE_LABEL.to_owned(), task.concurrent.as_slice()))
                    .chain(task.profiles.iter().filter_map(|(name, profile)| {
                        profile
                            .concurrent_entries()
                            .map(|entries| (name.clone(), entries))
                    }));
            for (profile, entries) in profiles {
                for entry in entries {
                    if entry.start_after_ms.is_none() {
                        continue;
                    }
                    let entry_name = entry
                        .name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                        .or_else(|| entry.task.clone())
                        .or_else(|| entry.run.clone())
                        .unwrap_or_else(|| "<unnamed>".to_owned());
                    retired.push((task_name.clone(), profile.clone(), entry_name));
                }
            }
        }
        retired
    }

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
            for name in catalog.sources.keys() {
                if name.trim().is_empty()
                    || name != name.trim()
                    || name.contains('/')
                    || name.contains(':')
                    || matches!(name.as_str(), "project" | "bundle" | "baseline")
                {
                    return Err(ManifestError::Compose {
                        path: manifest_path.to_path_buf(),
                        detail: format!(
                            "`[catalog.sources.{name}]` needs a plain handle: no '/' or ':', and not `project`, `bundle` or `baseline`, which name the built-in catalog sources"
                        ),
                    });
                }
            }
        }
        for (task_name, task) in &self.tasks {
            if task
                .group
                .as_ref()
                .is_some_and(|group| group.trim().is_empty())
            {
                return Err(ManifestError::Compose {
                    path: manifest_path.to_path_buf(),
                    detail: format!("task `{task_name}` group must be a non-empty string"),
                });
            }
            validate_task_description(manifest_path, task_name, task.description.as_deref())?;
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
            validate_container_data_from(manifest_path, containers)?;
        }
        Ok(())
    }
}

/// Validate an optional task `description`: one non-empty line of at most
/// [`TASK_DESCRIPTION_MAX_CHARS`] characters.
fn validate_task_description(
    manifest_path: &Path,
    task_name: &str,
    description: Option<&str>,
) -> Result<(), ManifestError> {
    let Some(description) = description else {
        return Ok(());
    };
    let reject = |detail: String| {
        Err(ManifestError::Compose {
            path: manifest_path.to_path_buf(),
            detail,
        })
    };
    if description.trim().is_empty() {
        return reject(format!(
            "task `{task_name}` description must be a non-empty string; remove the key or write one line of context"
        ));
    }
    if description.contains('\n') || description.contains('\r') {
        return reject(format!(
            "task `{task_name}` description must be a single line; move the detail into the task's run command or a guide"
        ));
    }
    let length = description.chars().count();
    if length > TASK_DESCRIPTION_MAX_CHARS {
        return reject(format!(
            "task `{task_name}` description is {length} characters; keep it to {TASK_DESCRIPTION_MAX_CHARS} or fewer so `effigy tasks` stays readable"
        ));
    }
    Ok(())
}

/// Validate every `[containers.<env>].data_from` reference at manifest load.
///
/// A borrowing environment mounts another environment's service data volumes,
/// so a broken reference would silently split or merge the wrong data. Every
/// failure mode therefore refuses here with a message naming both
/// environments.
fn validate_container_data_from(
    manifest_path: &Path,
    containers: &ManifestContainersConfig,
) -> Result<(), ManifestError> {
    let compose_error = |detail: String| ManifestError::Compose {
        path: manifest_path.to_path_buf(),
        detail,
    };
    for (name, config) in &containers.environments {
        let Some(target) = config.data_from.as_deref().map(str::trim) else {
            continue;
        };
        if target.is_empty() {
            return Err(compose_error(format!(
                "[containers.{name}].data_from must name another container environment; found an empty value"
            )));
        }
        if config.compose_file.is_some() {
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" requires catalog-driven `[containers.{name}.services]`; direct `compose_file` ownership declares volumes Effigy cannot resolve"
            )));
        }
        if config.share_runtime_identity {
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" cannot combine with `share_runtime_identity = true`; a borrowing environment keeps its own Compose project identity"
            )));
        }
        if target == name.as_str() {
            return Err(compose_error(format!(
                "[containers.{name}].data_from cannot name the environment itself"
            )));
        }
        let Some(owner) = containers.environments.get(target) else {
            let available = containers
                .environments
                .keys()
                .map(|entry| format!("`{entry}`"))
                .collect::<Vec<_>>()
                .join(", ");
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" names an unknown container environment (available: {available})"
            )));
        };
        if owner.compose_file.is_some() {
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" cannot borrow from a direct `compose_file` environment; the owner must declare `[containers.{target}.services]`"
            )));
        }
        if let Some(next) = owner
            .data_from
            .as_deref()
            .map(str::trim)
            .filter(|next| !next.is_empty())
        {
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" would chain through \"{next}\"; data_from must name an environment that owns its volumes directly"
            )));
        }
        let effective_driver = |config: &ManifestContainerConfig| {
            config.driver.unwrap_or(ManifestContainerDriver::Colima)
        };
        if effective_driver(config) != effective_driver(owner) {
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" requires both environments to use the same driver"
            )));
        }
        for (service_name, service) in &config.services {
            let Some(owner_service) = owner.services.get(service_name) else {
                continue;
            };
            if service.catalog == owner_service.catalog {
                continue;
            }
            if service.shared.unwrap_or(false) || owner_service.shared.unwrap_or(false) {
                continue;
            }
            return Err(compose_error(format!(
                "[containers.{name}].data_from = \"{target}\" cannot reconcile service `{service_name}`: it is `{}` in `{name}` but `{}` in `{target}`; give the service distinct names or match the catalog",
                service.catalog, owner_service.catalog
            )));
        }
    }
    Ok(())
}

fn validate_dns_routes(
    manifest_path: &Path,
    container_name: &str,
    dns: &crate::config_sections::ManifestContainerDnsConfig,
) -> Result<(), ManifestError> {
    if let Some(base_domain) = dns.base_domain.as_deref() {
        validate_dns_domain_name(
            manifest_path,
            base_domain,
            &format!("containers.{container_name}.dns.base_domain"),
        )?;
        if base_domain.trim().trim_end_matches('.').split('.').count() < 2 {
            return Err(ManifestError::Compose {
                path: manifest_path.to_path_buf(),
                detail: format!(
                    "containers.{container_name}.dns.base_domain must contain at least two labels"
                ),
            });
        }
    }
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
base_domain = "acme.test"
domains = ["a.test"]
domain_defaults = { tls = true, target_host = "127.0.0.1:8080" }
"#,
        );
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("expected valid manifest");
    }

    #[test]
    fn dns_base_domain_must_contain_at_least_two_labels() {
        let manifest = parse(
            r#"
[containers.web]
primary_service = "app"

[containers.web.dns]
base_domain = "localhost"
"#,
        );
        let detail = err(&manifest);
        assert!(
            detail.contains("base_domain must contain at least two labels"),
            "got: {detail}"
        );
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

#[cfg(test)]
mod container_data_from_validation_tests {
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

    fn valid_pair() -> String {
        r#"
[containers]
default = "dev"

[containers.dev]
primary_service = "app"

[containers.dev.services.db]
catalog = "postgres"

[containers.hybrid]
primary_service = "app"
data_from = "dev"

[containers.hybrid.services.db]
catalog = "postgres"
"#
        .to_owned()
    }

    #[test]
    fn valid_data_from_pair_is_accepted() {
        parse(&valid_pair())
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("valid data_from pair");
    }

    #[test]
    fn unknown_data_from_target_is_rejected_with_available_names() {
        let mut manifest = valid_pair();
        manifest = manifest.replace("data_from = \"dev\"", "data_from = \"missing\"");
        let error = err(&parse(&manifest));
        assert!(error.contains("unknown container environment"), "{error}");
        assert!(error.contains("`dev`"), "{error}");
    }

    #[test]
    fn self_referencing_data_from_is_rejected() {
        let manifest = valid_pair().replace("data_from = \"dev\"", "data_from = \"hybrid\"");
        let error = err(&parse(&manifest));
        assert!(
            error.contains("cannot name the environment itself"),
            "{error}"
        );
    }

    #[test]
    fn chained_data_from_is_rejected() {
        let manifest = format!(
            "{}\n[containers.chain]\nprimary_service = \"app\"\ndata_from = \"hybrid\"\n\n[containers.chain.services.db]\ncatalog = \"postgres\"\n",
            valid_pair()
        );
        let error = err(&parse(&manifest));
        assert!(error.contains("would chain through"), "{error}");
    }

    #[test]
    fn explicit_matching_driver_is_accepted() {
        let manifest = valid_pair().replace(
            "[containers.hybrid]\nprimary_service",
            "[containers.hybrid]\ndriver = \"colima\"\nprimary_service",
        );
        // Both environments resolve to the same effective driver, so the
        // explicit declaration stays valid. Only `ManifestContainerDriver`
        // variants that can diverge can produce a mismatch refusal.
        parse(&manifest)
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("matching explicit driver");
    }

    #[test]
    fn empty_data_from_value_is_rejected() {
        let manifest = valid_pair().replace("data_from = \"dev\"", "data_from = \" \"");
        let error = err(&parse(&manifest));
        assert!(
            error.contains("must name another container environment"),
            "{error}"
        );
    }

    #[test]
    fn data_from_with_direct_compose_file_is_rejected() {
        let manifest = valid_pair().replace(
            "[containers.hybrid]\nprimary_service = \"app\"",
            "[containers.hybrid]\nprimary_service = \"app\"\ncompose_file = \"docker-compose.yml\"",
        );
        let error = err(&parse(&manifest));
        assert!(error.contains("requires catalog-driven"), "{error}");
    }

    #[test]
    fn data_from_owner_with_direct_compose_file_is_rejected() {
        let manifest = valid_pair().replace(
            "[containers.dev]\nprimary_service = \"app\"",
            "[containers.dev]\nprimary_service = \"app\"\ncompose_file = \"docker-compose.yml\"",
        );
        let error = err(&parse(&manifest));
        assert!(error.contains("cannot borrow from a direct"), "{error}");
    }

    #[test]
    fn data_from_with_share_runtime_identity_is_rejected() {
        let manifest = valid_pair().replace(
            "[containers.hybrid]\nprimary_service = \"app\"",
            "[containers.hybrid]\nprimary_service = \"app\"\nshare_runtime_identity = true",
        );
        let error = err(&parse(&manifest));
        assert!(error.contains("share_runtime_identity"), "{error}");
    }

    #[test]
    fn conflicting_same_name_service_catalogs_are_rejected() {
        let manifest = valid_pair().replace(
            "[containers.hybrid.services.db]\ncatalog = \"postgres\"",
            "[containers.hybrid.services.db]\ncatalog = \"mariadb\"",
        );
        let error = err(&parse(&manifest));
        assert!(error.contains("cannot reconcile service `db`"), "{error}");
    }
}

#[cfg(test)]
mod top_level_keys_tests {
    use super::{TaskManifest, MANIFEST_TOP_LEVEL_KEYS, RETIRED_MANIFEST_TOP_LEVEL_KEYS};
    use std::collections::BTreeSet;

    #[test]
    fn top_level_keys_match_task_manifest_fields() {
        let error = toml::from_str::<TaskManifest>("__effigy_probe = 1")
            .expect_err("unknown top-level key must be rejected")
            .to_string();
        let expected = error
            .split_once("expected one of")
            .map(|(_, fields)| fields)
            .unwrap_or_else(|| panic!("unexpected serde error shape: {error}"));
        let fields: BTreeSet<&str> = expected.split('`').skip(1).step_by(2).collect();
        let declared: BTreeSet<&str> = MANIFEST_TOP_LEVEL_KEYS
            .iter()
            .copied()
            .filter(|key| *key != "manifest" && !RETIRED_MANIFEST_TOP_LEVEL_KEYS.contains(key))
            .collect();
        assert_eq!(declared, fields);
    }
}

#[cfg(test)]
mod task_description_validation_tests {
    use super::{TaskManifest, TASK_DESCRIPTION_MAX_CHARS};
    use std::path::Path;

    fn error_for(text: &str) -> String {
        let manifest: TaskManifest = toml::from_str(text).expect("parse manifest");
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect_err("description must be refused")
            .to_string()
    }

    #[test]
    fn an_empty_description_is_refused_with_the_fix() {
        let detail = error_for("[tasks.demo]\ndescription = \"   \"\nrun = \"printf demo\"\n");
        assert!(detail.contains("task `demo` description"), "{detail}");
        assert!(
            detail.contains("remove the key or write one line of context"),
            "{detail}"
        );
    }

    #[test]
    fn a_multi_line_description_is_refused() {
        let detail = error_for(
            "[tasks.demo]\ndescription = \"first line\\nsecond line\"\nrun = \"printf demo\"\n",
        );
        assert!(
            detail.contains("must be a single line"),
            "a multi-line description must name the rule: {detail}"
        );
    }

    #[test]
    fn a_description_over_the_limit_names_the_length_and_limit() {
        let too_long = "x".repeat(TASK_DESCRIPTION_MAX_CHARS + 1);
        let detail = error_for(&format!(
            "[tasks.demo]\ndescription = \"{too_long}\"\nrun = \"printf demo\"\n"
        ));
        assert!(
            detail.contains(&format!(
                "{} characters; keep it to {} or fewer",
                TASK_DESCRIPTION_MAX_CHARS + 1,
                TASK_DESCRIPTION_MAX_CHARS
            )),
            "{detail}"
        );
    }

    #[test]
    fn a_description_at_the_limit_and_a_unicode_one_are_accepted() {
        let at_limit = "é".repeat(TASK_DESCRIPTION_MAX_CHARS);
        let manifest: TaskManifest = toml::from_str(&format!(
            "[tasks.demo]\ndescription = \"{at_limit}\"\nrun = \"printf demo\"\n"
        ))
        .expect("parse manifest");
        manifest
            .validate(Path::new("/tmp/effigy.toml"))
            .expect("a 200-character description is accepted");

        assert_eq!(
            manifest.tasks["demo"].description.as_deref(),
            Some(at_limit.as_str())
        );
    }
}
