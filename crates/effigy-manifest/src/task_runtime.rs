use std::collections::BTreeMap;

use indexmap::IndexMap;

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestTask {
    #[serde(default)]
    pub run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
    #[serde(default)]
    pub system: Option<String>,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub stay_in_shell: Option<bool>,
    #[serde(default)]
    pub lock: Option<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub fail_on_non_zero: Option<bool>,
    #[serde(default)]
    pub container_lifecycle: Option<bool>,
    #[serde(default)]
    pub gateway: Option<bool>,
    #[serde(default)]
    pub health_wait: Option<bool>,
    #[serde(default)]
    pub ready_message: Option<String>,
    #[serde(default)]
    pub concurrent: Vec<ManifestManagedConcurrentEntry>,
    #[serde(default)]
    pub profiles: IndexMap<String, ManifestManagedProfile>,
    #[serde(default)]
    pub cache: Option<ManifestTaskCache>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestTaskRunIn {
    Host,
    Container,
    Either,
}

impl ManifestTask {
    pub fn run_in(&self) -> ManifestTaskRunIn {
        self.run_in.unwrap_or(ManifestTaskRunIn::Either)
    }

    pub fn effective_run_in(&self, default_run_in: Option<ManifestTaskRunIn>) -> ManifestTaskRunIn {
        self.run_in
            .or_else(|| {
                if self.container_lifecycle.unwrap_or(false) {
                    Some(ManifestTaskRunIn::Container)
                } else {
                    None
                }
            })
            .or(default_run_in)
            .unwrap_or(ManifestTaskRunIn::Either)
    }
}

impl ManifestTaskRunIn {
    pub fn as_str(self) -> &'static str {
        match self {
            ManifestTaskRunIn::Host => "host",
            ManifestTaskRunIn::Container => "container",
            ManifestTaskRunIn::Either => "either",
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestManagedConcurrentEntry {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub service: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub run: Option<String>,
    /// Per-entry execution context override. When the parent task
    /// binds to a container/workspace, individual concurrent entries
    /// can opt out of that wrap with `run_in = "host"` — useful for
    /// host-only sidecars (e.g. an `autossh` tunnel that depends on
    /// the developer's local ssh_config). Defaults to `Either`,
    /// meaning the entry inherits the parent task's execution context.
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
    #[serde(default)]
    pub setup: Vec<ManifestManagedRunStep>,
    #[serde(default)]
    pub start: Option<usize>,
    #[serde(default)]
    pub tab: Option<usize>,
    #[serde(default)]
    pub start_after_ms: Option<u64>,
    #[serde(default)]
    pub shutdown_on_exit: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestManagedRun {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
enum ManifestManagedRunDef {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
    Step(Box<ManifestManagedRunStepTable>),
}

impl<'de> serde::Deserialize<'de> for ManifestManagedRun {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        match ManifestManagedRunDef::deserialize(deserializer)? {
            ManifestManagedRunDef::Command(command) => Ok(Self::Command(command)),
            ManifestManagedRunDef::Sequence(steps) => Ok(Self::Sequence(steps)),
            ManifestManagedRunDef::Step(step) => {
                Ok(Self::Sequence(vec![ManifestManagedRunStep::Step(step)]))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestManagedRunStep {
    Command(String),
    Step(Box<ManifestManagedRunStepTable>),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestManagedRunStepTable {
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub rhai: Option<String>,
    #[serde(default)]
    pub env: Option<ManifestRunStepEnv>,
    #[serde(default)]
    pub env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub retry: Option<usize>,
    #[serde(default)]
    pub retry_delay_ms: Option<u64>,
    #[serde(default)]
    pub fail_fast: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestRunStepEnv {
    Inline(BTreeMap<String, String>),
    Profile(String),
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestEnvFileDirective {
    Single(String),
    Many(Vec<String>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestEnvEntry {
    Value(String),
    Profile(Vec<BTreeMap<String, String>>),
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestManagedProfile {
    #[serde(default)]
    pub concurrent: Vec<ManifestManagedConcurrentEntry>,
}

#[derive(Debug, Clone, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestTaskCache {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub env: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestInlineTaskDefinition {
    #[serde(default)]
    pub run: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub rhai: Option<String>,
    #[serde(default)]
    pub run_in: Option<ManifestTaskRunIn>,
}

impl ManifestInlineTaskDefinition {
    pub fn into_manifest_task(self) -> ManifestTask {
        ManifestTask {
            run: Some(ManifestManagedRun::Sequence(vec![
                ManifestManagedRunStep::Step(Box::new(ManifestManagedRunStepTable {
                    run: self.run,
                    task: self.task,
                    rhai: self.rhai,
                    env: None,
                    env_file: None,
                    id: None,
                    depends_on: Vec::new(),
                    timeout_ms: None,
                    retry: None,
                    retry_delay_ms: None,
                    fail_fast: None,
                })),
            ])),
            run_in: self.run_in,
            ..ManifestTask::default()
        }
    }
}

impl ManifestManagedProfile {
    pub fn concurrent_entries(&self) -> Option<&[ManifestManagedConcurrentEntry]> {
        if self.concurrent.is_empty() {
            None
        } else {
            Some(self.concurrent.as_slice())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ManifestManagedRun, ManifestManagedRunStep, ManifestTask};

    #[test]
    fn run_accepts_single_task_object_without_array_wrapper() {
        let task: ManifestTask = toml::from_str(
            r#"
run = { task = "docs check headings README.md --require-heading '# Hello'" }
"#,
        )
        .expect("parse task");

        let Some(ManifestManagedRun::Sequence(steps)) = task.run else {
            panic!("expected single task object to deserialize as one-step sequence");
        };
        assert!(matches!(
            steps.as_slice(),
            [ManifestManagedRunStep::Step(step)]
                if step.task.as_deref() == Some("docs check headings README.md --require-heading '# Hello'")
        ));
    }
}
