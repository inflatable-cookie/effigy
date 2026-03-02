use std::collections::BTreeMap;
use std::fmt;

use indexmap::IndexMap;
use serde::de::{self, SeqAccess, Visitor};

macro_rules! impl_deserialize_string_or_map {
    (
        $ty:ty,
        $visitor:ident,
        $expecting:literal,
        $string_variant:path,
        $map_ty:ty,
        $map_variant:path
    ) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct $visitor;

                impl<'de> Visitor<'de> for $visitor {
                    type Value = $ty;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str($expecting)
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        Ok($string_variant(value.to_owned()))
                    }

                    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        Ok($string_variant(value))
                    }

                    fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
                    where
                        M: de::MapAccess<'de>,
                    {
                        let value = <$map_ty as serde::Deserialize>::deserialize(
                            de::value::MapAccessDeserializer::new(map),
                        )?;
                        Ok($map_variant(value))
                    }
                }

                deserializer.deserialize_any($visitor)
            }
        }
    };
}

macro_rules! impl_deserialize_string_or_seq {
    (
        $ty:ty,
        $visitor:ident,
        $expecting:literal,
        $string_variant:path,
        $seq_ty:ty,
        $seq_variant:path
    ) => {
        impl<'de> serde::Deserialize<'de> for $ty {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct $visitor;

                impl<'de> Visitor<'de> for $visitor {
                    type Value = $ty;

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str($expecting)
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        Ok($string_variant(value.to_owned()))
                    }

                    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
                    where
                        E: de::Error,
                    {
                        Ok($string_variant(value))
                    }

                    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
                    where
                        A: SeqAccess<'de>,
                    {
                        let value = <$seq_ty as serde::Deserialize>::deserialize(
                            de::value::SeqAccessDeserializer::new(seq),
                        )?;
                        Ok($seq_variant(value))
                    }
                }

                deserializer.deserialize_any($visitor)
            }
        }
    };
}

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

#[derive(Debug)]
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

#[derive(Debug)]
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

impl_deserialize_string_or_map!(
    ManifestTestRunnerOverride,
    OverrideVisitor,
    "string command or table with `command` field",
    ManifestTestRunnerOverride::Command,
    ManifestTestRunnerOverrideTable,
    ManifestTestRunnerOverride::Config
);

impl_deserialize_string_or_map!(
    ManifestTestSuite,
    SuiteVisitor,
    "string command or table with `run` field",
    ManifestTestSuite::Command,
    ManifestTestSuiteTable,
    ManifestTestSuite::Config
);

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

#[derive(Debug)]
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

impl<'de> serde::Deserialize<'de> for ManifestTaskDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TaskDefinitionVisitor;

        impl<'de> Visitor<'de> for TaskDefinitionVisitor {
            type Value = ManifestTaskDefinition;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("string command, sequence of run steps, or full task table")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ManifestTaskDefinition::Run(value.to_owned()))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(ManifestTaskDefinition::Run(value))
            }

            fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let sequence = <Vec<ManifestManagedRunStep> as serde::Deserialize>::deserialize(
                    de::value::SeqAccessDeserializer::new(seq),
                )?;
                Ok(ManifestTaskDefinition::RunSequence(sequence))
            }

            fn visit_map<M>(self, map: M) -> Result<Self::Value, M::Error>
            where
                M: de::MapAccess<'de>,
            {
                let task = <ManifestTask as serde::Deserialize>::deserialize(
                    de::value::MapAccessDeserializer::new(map),
                )?;
                Ok(ManifestTaskDefinition::Full(Box::new(task)))
            }
        }

        deserializer.deserialize_any(TaskDefinitionVisitor)
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

#[derive(Debug)]
pub(super) enum ManifestManagedRun {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
}

#[derive(Debug)]
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

impl_deserialize_string_or_seq!(
    ManifestManagedRun,
    ManagedRunVisitor,
    "string command or sequence of run steps",
    ManifestManagedRun::Command,
    Vec<ManifestManagedRunStep>,
    ManifestManagedRun::Sequence
);

impl_deserialize_string_or_map!(
    ManifestManagedRunStep,
    ManagedRunStepVisitor,
    "string command or run-step table",
    ManifestManagedRunStep::Command,
    ManifestManagedRunStepTable,
    ManifestManagedRunStep::Step
);

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
