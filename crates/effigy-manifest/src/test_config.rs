use std::collections::BTreeMap;

use super::task_runtime::{
    ManifestEnvFileDirective, ManifestManagedRun, ManifestManagedRunStep, ManifestRunStepEnv,
};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestTestConfig {
    #[serde(default)]
    pub max_parallel: Option<usize>,
    #[serde(default)]
    pub cargo_env_match: ManifestCargoEnvMatchMode,
    #[serde(default)]
    pub runners: BTreeMap<String, ManifestTestRunnerOverride>,
    #[serde(default)]
    pub suites: BTreeMap<String, ManifestTestSuite>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestCargoEnvMatchMode {
    ExecutableOnly,
    #[default]
    PrefixAware,
    ShellAware,
}

impl ManifestCargoEnvMatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            ManifestCargoEnvMatchMode::ExecutableOnly => "executable-only",
            ManifestCargoEnvMatchMode::PrefixAware => "prefix-aware",
            ManifestCargoEnvMatchMode::ShellAware => "shell-aware",
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestTestRunnerOverride {
    Command(String),
    Config(ManifestTestRunnerOverrideTable),
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ManifestTestRunnerOverrideTable {
    #[serde(default)]
    pub command: Option<String>,
}

impl ManifestTestRunnerOverride {
    pub fn command(&self) -> Option<&str> {
        match self {
            ManifestTestRunnerOverride::Command(command) => Some(command.as_str()),
            ManifestTestRunnerOverride::Config(table) => table.command.as_deref(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub enum ManifestTestSuite {
    Command(String),
    Config(ManifestTestSuiteTable),
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestTestSuiteTable {
    pub run: ManifestManagedRun,
    #[serde(default)]
    pub env: Option<ManifestRunStepEnv>,
    #[serde(default)]
    pub env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub setup: Vec<ManifestManagedRunStep>,
    #[serde(default)]
    pub teardown: Vec<ManifestManagedRunStep>,
    #[serde(default)]
    pub teardown_policy: ManifestTestSuiteTeardownPolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ManifestTestSuiteTeardownPolicy {
    Always,
    #[default]
    OnSuccess,
}

#[allow(dead_code)]
impl ManifestTestSuite {
    pub fn command(&self) -> Option<&str> {
        match self {
            ManifestTestSuite::Command(command) => Some(command.as_str()),
            ManifestTestSuite::Config(_) => None,
        }
    }

    pub fn managed_run(&self) -> Option<&ManifestManagedRun> {
        match self {
            ManifestTestSuite::Command(_) => None,
            ManifestTestSuite::Config(table) => Some(&table.run),
        }
    }

    pub fn env(&self) -> Option<&ManifestRunStepEnv> {
        match self {
            ManifestTestSuite::Command(_) => None,
            ManifestTestSuite::Config(table) => table.env.as_ref(),
        }
    }

    pub fn env_file(&self) -> Option<&ManifestEnvFileDirective> {
        match self {
            ManifestTestSuite::Command(_) => None,
            ManifestTestSuite::Config(table) => table.env_file.as_ref(),
        }
    }

    pub fn setup(&self) -> &[ManifestManagedRunStep] {
        match self {
            ManifestTestSuite::Command(_) => &[],
            ManifestTestSuite::Config(table) => table.setup.as_slice(),
        }
    }

    pub fn teardown(&self) -> &[ManifestManagedRunStep] {
        match self {
            ManifestTestSuite::Command(_) => &[],
            ManifestTestSuite::Config(table) => table.teardown.as_slice(),
        }
    }

    pub fn teardown_policy(&self) -> ManifestTestSuiteTeardownPolicy {
        match self {
            ManifestTestSuite::Command(_) => ManifestTestSuiteTeardownPolicy::OnSuccess,
            ManifestTestSuite::Config(table) => table.teardown_policy,
        }
    }
}
