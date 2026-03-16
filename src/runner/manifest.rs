use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

#[path = "manifest/config_sections.rs"]
pub(in crate::runner) mod config_sections;
#[path = "manifest/task_defs.rs"]
mod task_defs;
#[path = "manifest/task_runtime.rs"]
pub(in crate::runner) mod task_runtime;
#[path = "manifest/test_config.rs"]
mod test_config;

pub(super) use config_sections::{
    ManifestDocsPolicyConfig, ManifestEnvSchemaConfig, ManifestPackageManagerConfig,
    ManifestReleaseConfig, ManifestScanConfig, ManifestShellConfig,
};
use task_defs::deserialize_tasks;
pub(super) use task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRun, ManifestManagedRunStep,
    ManifestTask,
};
use test_config::ManifestTestConfig;
pub(super) use test_config::{ManifestCargoEnvMatchMode, ManifestTestSuiteTeardownPolicy};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskManifest {
    #[serde(default)]
    pub(super) catalog: Option<ManifestCatalog>,
    #[serde(default)]
    pub(super) defer: Option<ManifestDefer>,
    #[serde(default)]
    pub(super) env: BTreeMap<String, ManifestEnvEntry>,
    #[serde(default)]
    pub(super) test: Option<ManifestTestConfig>,
    #[serde(default)]
    pub(super) package_manager: Option<ManifestPackageManagerConfig>,
    #[serde(default)]
    pub(super) scan: Option<ManifestScanConfig>,
    #[serde(default)]
    pub(super) shell: Option<ManifestShellConfig>,
    #[serde(default)]
    pub(super) env_schema: Option<ManifestEnvSchemaConfig>,
    #[serde(default)]
    pub(super) docs_policy: Option<ManifestDocsPolicyConfig>,
    #[serde(default)]
    pub(super) release: Option<ManifestReleaseConfig>,
    #[serde(default, deserialize_with = "deserialize_tasks")]
    pub(super) tasks: BTreeMap<String, ManifestTask>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestCatalog {
    pub(super) alias: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestDefer {
    pub(super) run: String,
    #[serde(default)]
    pub(super) builtins: Vec<String>,
}

pub(in crate::runner) fn load_task_manifest(
    manifest_path: &Path,
) -> Result<TaskManifest, super::RunnerError> {
    let manifest_src = std::fs::read_to_string(manifest_path).map_err(|error| {
        super::RunnerError::TaskManifestRead {
            path: manifest_path.to_path_buf(),
            error,
        }
    })?;
    toml::from_str(&manifest_src).map_err(|error| super::RunnerError::TaskManifestParse {
        path: manifest_path.to_path_buf(),
        error,
    })
}

impl ManifestDefer {
    pub(in crate::runner) fn explicitly_deferred_builtins(&self) -> BTreeSet<String> {
        self.builtins
            .iter()
            .map(|name| name.trim())
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .collect::<BTreeSet<String>>()
    }
}
