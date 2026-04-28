use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

mod bundles;
mod composition;
pub mod config_sections;
pub mod execution_binding;
mod loaded_catalog;
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
    export_bundle, get_bundle, list_bundle_default_paths, list_bundles, render_bundle_defaults,
    BundleExport, BundleInputSpec, BundleInputType, BundleSpec,
};
pub use composition::{
    load_task_manifest_with_inspection, LoadedTaskManifest, ManifestCompositionEdge,
    ManifestCompositionOverride, ManifestCompositionValueSource,
};
pub use config_sections::{
    ManifestBootstrapConfig, ManifestBootstrapSubmodulesPolicy, ManifestBundleConfig,
    ManifestContainerConfig, ManifestContainerDataConfig, ManifestContainerDnsConfig,
    ManifestContainerDnsDomainDefaults, ManifestContainerDnsRouteConfig, ManifestContainerDriver,
    ManifestContainerExecAliasConfig, ManifestContainerExecAliasTableConfig,
    ManifestContainerOnTaskExit, ManifestContainerServiceConfig, ManifestContainerShutdownMode,
    ManifestContainerStartup, ManifestContainersConfig, ManifestDemoConfig, ManifestDemoMode,
    ManifestDemoStatus, ManifestDistributionConfig, ManifestDistributionMetadataConfig,
    ManifestDistributionPackageConfig, ManifestDistributionPreflightConfig,
    ManifestDocsPolicyConfig, ManifestEnvSchemaConfig, ManifestInlineWorkspaceContainerConfig,
    ManifestIsolationAdoption, ManifestIsolationConfig, ManifestJsPackageManager,
    ManifestPackageManagerConfig, ManifestReleaseConfig, ManifestScanConfig, ManifestShellConfig,
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
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedConcurrentEntry,
    ManifestManagedProfile, ManifestManagedRun, ManifestManagedRunStep,
    ManifestManagedRunStepTable, ManifestRunStepEnv, ManifestTask, ManifestTaskCache,
    ManifestTaskRunIn,
};
use test_config::ManifestTestConfig;
pub use test_config::{ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};
pub use user_config::{
    load_user_config, load_user_config_from, with_test_user_config_home, LibraryMount,
    UserBundleConfig, UserConfig, USER_CONFIG_FILE,
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
        Ok(())
    }
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
