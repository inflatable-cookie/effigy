use std::collections::BTreeMap;

use indexmap::IndexMap;

#[path = "manifest/task_defs.rs"]
mod task_defs;
#[path = "manifest/test_config.rs"]
mod test_config;

use task_defs::deserialize_tasks;
pub(super) use test_config::ManifestCargoEnvMatchMode;
use test_config::ManifestTestConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ManifestScanOutputFormat {
    Text,
    Markdown,
}

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
pub(super) struct ManifestShellConfig {
    #[serde(default)]
    pub(super) run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub(super) js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestScanConfig {
    #[serde(default)]
    pub(super) god_files: Option<ManifestGodFilesConfig>,
    #[serde(default)]
    pub(super) generated_assets: Option<ManifestGeneratedAssetsConfig>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestGodFilesConfig {
    #[serde(default, alias = "threshold")]
    pub(super) warn: Option<usize>,
    #[serde(default)]
    pub(super) high: Option<usize>,
    #[serde(default)]
    pub(super) critical: Option<usize>,
    #[serde(default)]
    pub(super) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(super) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(super) doctor: Option<bool>,
    #[serde(default)]
    pub(super) include: Vec<String>,
    #[serde(default)]
    pub(super) exclude: Vec<String>,
    #[serde(default)]
    pub(super) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(super) out: Option<String>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestGeneratedAssetsConfig {
    #[serde(default, alias = "threshold")]
    pub(super) warn: Option<usize>,
    #[serde(default)]
    pub(super) high: Option<usize>,
    #[serde(default)]
    pub(super) critical: Option<usize>,
    #[serde(default)]
    pub(super) fail_on_findings: Option<bool>,
    #[serde(default)]
    pub(super) respect_gitignore: Option<bool>,
    #[serde(default)]
    pub(super) doctor: Option<bool>,
    #[serde(default)]
    pub(super) include: Vec<String>,
    #[serde(default)]
    pub(super) exclude: Vec<String>,
    #[serde(default)]
    pub(super) format: Option<ManifestScanOutputFormat>,
    #[serde(default)]
    pub(super) out: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTask {
    #[serde(default)]
    pub(super) run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub(super) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(super) env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub(super) mode: Option<String>,
    #[serde(default)]
    pub(super) fail_on_non_zero: Option<bool>,
    #[serde(default)]
    pub(super) concurrent: Vec<ManifestManagedConcurrentEntry>,
    #[serde(default)]
    pub(super) profiles: IndexMap<String, ManifestManagedProfile>,
    #[serde(default)]
    pub(super) cache: Option<ManifestTaskCache>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestManagedConcurrentEntry {
    #[serde(default)]
    pub(super) name: Option<String>,
    #[serde(default)]
    pub(super) task: Option<String>,
    #[serde(default)]
    pub(super) run: Option<String>,
    #[serde(default)]
    pub(super) start: Option<usize>,
    #[serde(default)]
    pub(super) tab: Option<usize>,
    #[serde(default)]
    pub(super) start_after_ms: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestManagedRun {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestManagedRunStep {
    Command(String),
    Step(ManifestManagedRunStepTable),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestManagedRunStepTable {
    #[serde(default)]
    pub(super) run: Option<String>,
    #[serde(default)]
    pub(super) task: Option<String>,
    #[serde(default)]
    pub(super) env: Option<ManifestRunStepEnv>,
    #[serde(default)]
    pub(super) env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) depends_on: Vec<String>,
    #[serde(default)]
    pub(super) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(super) retry: Option<usize>,
    #[serde(default)]
    pub(super) retry_delay_ms: Option<u64>,
    #[serde(default)]
    pub(super) fail_fast: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestRunStepEnv {
    Inline(BTreeMap<String, String>),
    Profile(String),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestEnvFileDirective {
    Single(String),
    Many(Vec<String>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestEnvEntry {
    Value(String),
    Profile(Vec<BTreeMap<String, String>>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestManagedProfile {
    #[serde(default)]
    pub(super) concurrent: Vec<ManifestManagedConcurrentEntry>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTaskCache {
    #[serde(default)]
    pub(super) enabled: bool,
    #[serde(default)]
    pub(super) inputs: Vec<String>,
    #[serde(default)]
    pub(super) outputs: Vec<String>,
    #[serde(default)]
    pub(super) env: Vec<String>,
}

impl ManifestManagedProfile {
    pub(super) fn concurrent_entries(&self) -> Option<&[ManifestManagedConcurrentEntry]> {
        if self.concurrent.is_empty() {
            None
        } else {
            Some(self.concurrent.as_slice())
        }
    }
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
