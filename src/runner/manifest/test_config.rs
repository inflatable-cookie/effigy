use std::collections::BTreeMap;

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

#[derive(Debug, serde::Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(in crate::runner) struct ManifestTestSuiteTable {
    pub(in crate::runner) run: String,
}

impl ManifestTestSuite {
    pub(in crate::runner) fn run(&self) -> Option<&str> {
        match self {
            ManifestTestSuite::Command(command) => Some(command.as_str()),
            ManifestTestSuite::Config(table) => Some(table.run.as_str()),
        }
    }
}
