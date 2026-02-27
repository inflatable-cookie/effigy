use std::collections::{BTreeMap, HashMap};

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
pub(super) struct ManifestShellConfig {
    #[serde(default)]
    pub(super) run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ManifestTestConfig {
    #[serde(default)]
    pub(super) max_parallel: Option<usize>,
    #[serde(default)]
    pub(super) runners: BTreeMap<String, ManifestTestRunnerOverride>,
}

#[derive(Debug, serde::Deserialize)]
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

#[derive(Debug, serde::Deserialize, Default)]
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
    pub(super) processes: BTreeMap<String, ManifestManagedProcess>,
    #[serde(default)]
    pub(super) profiles: BTreeMap<String, ManifestManagedProfile>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestTaskDefinition {
    Run(String),
    RunSequence(Vec<ManifestManagedRunStep>),
    Full(ManifestTask),
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
            ManifestTaskDefinition::Full(task) => task,
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ManifestManagedProcess {
    #[serde(default)]
    pub(super) run: Option<ManifestManagedRun>,
    #[serde(default)]
    pub(super) task: Option<String>,
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
pub(super) struct ManifestManagedRunStepTable {
    #[serde(default)]
    pub(super) run: Option<String>,
    #[serde(default)]
    pub(super) task: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestManagedProfile {
    Table(ManifestManagedProfileTable),
    List(Vec<String>),
    Ranked(BTreeMap<String, ManifestManagedProfileOrder>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManifestManagedProfileTable {
    #[serde(default)]
    pub(super) processes: Vec<String>,
    #[serde(default)]
    pub(super) start: Vec<String>,
    #[serde(default)]
    pub(super) tabs: Option<ManifestManagedTabOrder>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestManagedProfileOrder {
    Rank(usize),
    Axes {
        #[serde(default)]
        start: Option<usize>,
        #[serde(default)]
        tab: Option<usize>,
        #[serde(default)]
        start_after_ms: Option<u64>,
    },
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
pub(super) enum ManifestManagedTabOrder {
    List(Vec<String>),
    Ranked(BTreeMap<String, usize>),
}

impl ManifestManagedProfile {
    pub(super) fn start_entries(&self) -> Vec<String> {
        match self {
            ManifestManagedProfile::Table(table) => {
                if table.start.is_empty() {
                    table.processes.clone()
                } else {
                    table.start.clone()
                }
            }
            ManifestManagedProfile::List(entries) => entries.clone(),
            ManifestManagedProfile::Ranked(ranked) => {
                let mut entries = ranked
                    .iter()
                    .map(|(name, order)| (name.clone(), order.start_rank()))
                    .collect::<Vec<(String, usize)>>();
                entries.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                entries.into_iter().map(|(name, _)| name).collect()
            }
        }
    }

    pub(super) fn tab_entries(&self) -> Option<Vec<String>> {
        match self {
            ManifestManagedProfile::Table(table) => tab_entries_from_order(table.tabs.as_ref()),
            ManifestManagedProfile::List(_) => None,
            ManifestManagedProfile::Ranked(ranked) => {
                let mut entries = ranked
                    .iter()
                    .map(|(name, order)| (name.clone(), order.tab_rank()))
                    .collect::<Vec<(String, usize)>>();
                if entries.is_empty() {
                    return None;
                }
                entries.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
                Some(entries.into_iter().map(|(name, _)| name).collect())
            }
        }
    }

    pub(super) fn start_delay_ms(&self) -> HashMap<String, u64> {
        match self {
            ManifestManagedProfile::Ranked(ranked) => ranked
                .iter()
                .filter_map(|(name, order)| {
                    order.start_delay_ms().map(|delay| (name.clone(), delay))
                })
                .collect(),
            _ => HashMap::new(),
        }
    }
}

impl ManifestManagedProfileOrder {
    fn start_rank(&self) -> usize {
        match self {
            ManifestManagedProfileOrder::Rank(rank) => *rank,
            ManifestManagedProfileOrder::Axes { start, tab, .. } => {
                start.or(*tab).unwrap_or(usize::MAX)
            }
        }
    }

    fn tab_rank(&self) -> usize {
        match self {
            ManifestManagedProfileOrder::Rank(rank) => *rank,
            ManifestManagedProfileOrder::Axes { start, tab, .. } => {
                tab.or(*start).unwrap_or(usize::MAX)
            }
        }
    }

    fn start_delay_ms(&self) -> Option<u64> {
        match self {
            ManifestManagedProfileOrder::Rank(_) => None,
            ManifestManagedProfileOrder::Axes { start_after_ms, .. } => *start_after_ms,
        }
    }
}

fn tab_entries_from_order(tabs: Option<&ManifestManagedTabOrder>) -> Option<Vec<String>> {
    match tabs {
        Some(ManifestManagedTabOrder::List(entries)) if !entries.is_empty() => {
            Some(entries.clone())
        }
        Some(ManifestManagedTabOrder::Ranked(rankings)) if !rankings.is_empty() => {
            let mut ordered = rankings
                .iter()
                .map(|(name, rank)| (name.clone(), *rank))
                .collect::<Vec<(String, usize)>>();
            ordered.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            Some(ordered.into_iter().map(|(name, _)| name).collect())
        }
        _ => None,
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
pub(super) struct ManifestCatalog {
    pub(super) alias: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
pub(super) struct ManifestDefer {
    pub(super) run: String,
}
