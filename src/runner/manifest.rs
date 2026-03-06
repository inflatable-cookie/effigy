use std::collections::BTreeMap;

#[path = "manifest/config_sections.rs"]
pub(in crate::runner) mod config_sections;
#[path = "manifest/task_defs.rs"]
mod task_defs;
#[path = "manifest/task_runtime.rs"]
pub(in crate::runner) mod task_runtime;
#[path = "manifest/test_config.rs"]
mod test_config;

pub(super) use config_sections::{
    ManifestPackageManagerConfig, ManifestScanConfig, ManifestShellConfig,
};
use task_defs::deserialize_tasks;
pub(super) use task_runtime::{
    ManifestEnvEntry, ManifestEnvFileDirective, ManifestManagedRun, ManifestManagedRunStep,
    ManifestTask,
};
pub(super) use test_config::ManifestCargoEnvMatchMode;
use test_config::ManifestTestConfig;

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
}
