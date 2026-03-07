use std::collections::BTreeMap;

use super::task_runtime::{ManifestEnvFileDirective, ManifestManagedRunStep, ManifestRunStepEnv};

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTestConfig {
    #[serde(default)]
    pub(in crate::runner) max_parallel: Option<usize>,
    #[serde(default)]
    pub(in crate::runner) cargo_env_match: ManifestCargoEnvMatchMode,
    #[serde(default)]
    pub(in crate::runner) runners: BTreeMap<String, ManifestTestRunnerOverride>,
    #[serde(default)]
    pub(in crate::runner) suites: BTreeMap<String, ManifestTestSuite>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum ManifestCargoEnvMatchMode {
    ExecutableOnly,
    #[default]
    PrefixAware,
    ShellAware,
}

impl ManifestCargoEnvMatchMode {
    pub(in crate::runner) fn as_str(self) -> &'static str {
        match self {
            ManifestCargoEnvMatchMode::ExecutableOnly => "executable-only",
            ManifestCargoEnvMatchMode::PrefixAware => "prefix-aware",
            ManifestCargoEnvMatchMode::ShellAware => "shell-aware",
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestTestRunnerOverride {
    Command(String),
    Config(ManifestTestRunnerOverrideTable),
}

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTestRunnerOverrideTable {
    #[serde(default)]
    pub(in crate::runner) command: Option<String>,
}

impl ManifestTestRunnerOverride {
    pub(in crate::runner) fn command(&self) -> Option<&str> {
        match self {
            ManifestTestRunnerOverride::Command(command) => Some(command.as_str()),
            ManifestTestRunnerOverride::Config(table) => table.command.as_deref(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(in crate::runner) enum ManifestTestSuite {
    Command(String),
    Config(ManifestTestSuiteTable),
}

#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTestSuiteTable {
    pub(in crate::runner) run: String,
    #[serde(default)]
    pub(in crate::runner) env: Option<ManifestRunStepEnv>,
    #[serde(default)]
    pub(in crate::runner) env_file: Option<ManifestEnvFileDirective>,
    #[serde(default)]
    pub(in crate::runner) setup: Vec<ManifestManagedRunStep>,
    #[serde(default)]
    pub(in crate::runner) teardown: Vec<ManifestManagedRunStep>,
    #[serde(default)]
    pub(in crate::runner) teardown_policy: ManifestTestSuiteTeardownPolicy,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(in crate::runner) enum ManifestTestSuiteTeardownPolicy {
    Always,
    #[default]
    OnSuccess,
}

#[allow(dead_code)]
impl ManifestTestSuite {
    pub(in crate::runner) fn run(&self) -> Option<&str> {
        match self {
            ManifestTestSuite::Command(command) => Some(command.as_str()),
            ManifestTestSuite::Config(table) => Some(table.run.as_str()),
        }
    }

    pub(in crate::runner) fn env(&self) -> Option<&ManifestRunStepEnv> {
        match self {
            ManifestTestSuite::Command(_) => None,
            ManifestTestSuite::Config(table) => table.env.as_ref(),
        }
    }

    pub(in crate::runner) fn env_file(&self) -> Option<&ManifestEnvFileDirective> {
        match self {
            ManifestTestSuite::Command(_) => None,
            ManifestTestSuite::Config(table) => table.env_file.as_ref(),
        }
    }

    pub(in crate::runner) fn setup(&self) -> &[ManifestManagedRunStep] {
        match self {
            ManifestTestSuite::Command(_) => &[],
            ManifestTestSuite::Config(table) => table.setup.as_slice(),
        }
    }

    pub(in crate::runner) fn teardown(&self) -> &[ManifestManagedRunStep] {
        match self {
            ManifestTestSuite::Command(_) => &[],
            ManifestTestSuite::Config(table) => table.teardown.as_slice(),
        }
    }

    pub(in crate::runner) fn teardown_policy(&self) -> ManifestTestSuiteTeardownPolicy {
        match self {
            ManifestTestSuite::Command(_) => ManifestTestSuiteTeardownPolicy::OnSuccess,
            ManifestTestSuite::Config(table) => table.teardown_policy,
        }
    }
}
