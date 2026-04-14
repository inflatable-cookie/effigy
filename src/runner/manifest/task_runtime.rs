use std::collections::BTreeMap;

use indexmap::IndexMap;

use crate::runner::locking::model::LockScope;

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTask {
    #[serde(default)]
    pub(in crate::runner) run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub(in crate::runner) lock: Option<String>,
    #[serde(default)]
    pub(in crate::runner) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(in crate::runner) env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub(in crate::runner) mode: Option<String>,
    #[serde(default)]
    pub(in crate::runner) fail_on_non_zero: Option<bool>,
    #[serde(default)]
    pub(in crate::runner) concurrent: Vec<ManifestManagedConcurrentEntry>,
    #[serde(default)]
    pub(in crate::runner) profiles: IndexMap<String, ManifestManagedProfile>,
    #[serde(default)]
    pub(in crate::runner) cache: Option<ManifestTaskCache>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestManagedConcurrentEntry {
    #[serde(default)]
    pub(in crate::runner) name: Option<String>,
    #[serde(default)]
    pub(in crate::runner) task: Option<String>,
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
    #[serde(default)]
    pub(in crate::runner) start: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) tab: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) start_after_ms: Option<u64>,
    #[serde(default)]
    pub(in crate::runner) shutdown_on_exit: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestManagedRun {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestManagedRunStep {
    Command(String),
    Step(Box<ManifestManagedRunStepTable>),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestManagedRunStepTable {
    #[serde(default)]
    pub(in crate::runner) run: Option<String>,
    #[serde(default)]
    pub(in crate::runner) task: Option<String>,
    #[serde(default)]
    pub(in crate::runner) rhai: Option<String>,
    #[serde(default)]
    pub(in crate::runner) rhai_file: Option<String>,
    #[serde(default)]
    pub(in crate::runner) env: Option<ManifestRunStepEnv>,
    #[serde(default)]
    pub(in crate::runner) env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub(in crate::runner) id: Option<String>,
    #[serde(default)]
    pub(in crate::runner) depends_on: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) timeout_ms: Option<u64>,
    #[serde(default)]
    pub(in crate::runner) retry: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) retry_delay_ms: Option<u64>,
    #[serde(default)]
    pub(in crate::runner) fail_fast: Option<bool>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestRunStepEnv {
    Inline(BTreeMap<String, String>),
    Profile(String),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestEnvFileDirective {
    Single(String),
    Many(Vec<String>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestEnvEntry {
    Value(String),
    Profile(Vec<BTreeMap<String, String>>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestManagedProfile {
    #[serde(default)]
    pub(in crate::runner) concurrent: Vec<ManifestManagedConcurrentEntry>,
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTaskCache {
    #[serde(default)]
    pub(in crate::runner) enabled: bool,
    #[serde(default)]
    pub(in crate::runner) inputs: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) outputs: Vec<String>,
    #[serde(default)]
    pub(in crate::runner) env: Vec<String>,
}

impl ManifestManagedProfile {
    pub(in crate::runner) fn concurrent_entries(
        &self,
    ) -> Option<&[ManifestManagedConcurrentEntry]> {
        if self.concurrent.is_empty() {
            None
        } else {
            Some(self.concurrent.as_slice())
        }
    }
}

impl ManifestTask {
    pub(in crate::runner) fn lock_scope(&self, task_name: &str) -> LockScope {
        match self.lock.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => LockScope::Shared(name.to_owned()),
            _ => LockScope::Task(task_name.to_owned()),
        }
    }
}
