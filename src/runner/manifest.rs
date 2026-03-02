use std::collections::BTreeMap;

use indexmap::IndexMap;

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TaskManifest {
    #[serde(default)]
    pub(super) catalog: Option<ManifestCatalog>,
    #[serde(default)]
    pub(super) defer: Option<ManifestDefer>,
    #[serde(default)]
    pub(super) test: Option<ManifestTestConfig>,
    #[serde(default)]
    pub(super) package_manager: Option<ManifestPackageManagerConfig>,
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
pub(super) struct ManifestTestConfig {
    #[serde(default)]
    pub(super) max_parallel: Option<usize>,
    #[serde(default)]
    pub(super) runners: BTreeMap<String, ManifestTestRunnerOverride>,
    #[serde(default)]
    pub(super) suites: BTreeMap<String, ManifestTestSuite>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestPackageManagerConfig {
    #[serde(default, alias = "js_ts", alias = "typescript")]
    pub(super) js: Option<ManifestJsPackageManager>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestTestRunnerOverride {
    Command(String),
    Config(ManifestTestRunnerOverrideTable),
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTestRunnerOverrideTable {
    #[serde(default)]
    pub(super) command: Option<String>,
}

impl ManifestTestRunnerOverride {
    pub(super) fn command(&self) -> Option<&str> {
        match self {
            ManifestTestRunnerOverride::Command(command) => Some(command.as_str()),
            ManifestTestRunnerOverride::Config(table) => table.command.as_deref(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestTestSuite {
    Command(String),
    Config(ManifestTestSuiteTable),
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTestSuiteTable {
    pub(super) run: String,
}

impl ManifestTestSuite {
    pub(super) fn run(&self) -> Option<&str> {
        match self {
            ManifestTestSuite::Command(command) => Some(command.as_str()),
            ManifestTestSuite::Config(table) => Some(table.run.as_str()),
        }
    }
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestTask {
    #[serde(default)]
    pub(super) run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub(super) mode: Option<String>,
    #[serde(default)]
    pub(super) fail_on_non_zero: Option<bool>,
    #[serde(default)]
    pub(super) shell: Option<bool>,
    #[serde(default)]
    pub(super) concurrent: Vec<ManifestManagedConcurrentEntry>,
    #[serde(default)]
    pub(super) profiles: IndexMap<String, ManifestManagedProfile>,
    #[serde(default)]
    pub(super) cache: Option<ManifestTaskCache>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestTaskDefinition {
    Run(String),
    RunSequence(Vec<ManifestManagedRunStep>),
    Full(Box<ManifestTask>),
}

impl ManifestTaskDefinition {
    fn into_manifest_task(self) -> ManifestTask {
        match self {
            ManifestTaskDefinition::Run(command) => ManifestTask {
                run: Some(ManifestManagedRun::Command(command)),
                ..ManifestTask::default()
            },
            ManifestTaskDefinition::RunSequence(sequence) => ManifestTask {
                run: Some(ManifestManagedRun::Sequence(sequence)),
                ..ManifestTask::default()
            },
            ManifestTaskDefinition::Full(task) => *task,
        }
    }
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

fn deserialize_tasks<'de, D>(deserializer: D) -> Result<BTreeMap<String, ManifestTask>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let definitions =
        <BTreeMap<String, ManifestTaskDefinition> as serde::Deserialize>::deserialize(
            deserializer,
        )?;
    Ok(definitions
        .into_iter()
        .map(|(name, definition)| (name, definition.into_manifest_task()))
        .collect())
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
