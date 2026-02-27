use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use crate::process_manager::ProcessManagerError;
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::pulse::PulseTask;
use crate::tasks::{Task, TaskContext, TaskError};
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
use crate::{Command, PulseArgs, TaskInvocation, TasksArgs};

mod builtin;
mod catalog;
mod deferral;
mod managed;

use builtin::try_run_builtin_task;
use catalog::{discover_catalogs, select_catalog_and_task};
use deferral::{run_deferred_request, select_deferral, should_attempt_deferral};
use managed::{render_task_run_spec, resolve_managed_task_plan, run_or_render_managed_task};

#[derive(Debug)]
pub enum RunnerError {
    Cwd(std::io::Error),
    Resolve(ResolveError),
    Task(TaskError),
    Ui(String),
    TaskInvocation(String),
    TaskCatalogsMissing {
        root: PathBuf,
    },
    TaskCatalogReadDir {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestRead {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestParse {
        path: PathBuf,
        error: toml::de::Error,
    },
    TaskCatalogAliasConflict {
        alias: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    TaskCatalogPrefixNotFound {
        prefix: String,
        available: Vec<String>,
    },
    TaskNotFound {
        name: String,
        path: PathBuf,
    },
    TaskNotFoundAny {
        name: String,
        catalogs: Vec<String>,
    },
    TaskAmbiguous {
        name: String,
        candidates: Vec<String>,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
    TaskCommandFailure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    ManagedProcess(ProcessManagerError),
    TaskManagedUnsupportedMode {
        task: String,
        mode: String,
    },
    TaskManagedProfileNotFound {
        task: String,
        profile: String,
        available: Vec<String>,
    },
    TaskManagedProfileEmpty {
        task: String,
        profile: String,
    },
    TaskManagedProcessNotFound {
        task: String,
        profile: String,
        process: String,
    },
    TaskManagedProcessInvalidDefinition {
        task: String,
        process: String,
        detail: String,
    },
    TaskManagedProfileTabOrderInvalid {
        task: String,
        profile: String,
        detail: String,
    },
    TaskManagedTaskReferenceInvalid {
        task: String,
        process: String,
        reference: String,
        detail: String,
    },
    TaskManagedNonZeroExit {
        task: String,
        profile: String,
        processes: Vec<(String, String)>,
    },
    TaskMissingRunCommand {
        task: String,
        path: PathBuf,
    },
    BuiltinTestNonZero {
        failures: Vec<(String, Option<i32>)>,
        rendered: String,
    },
    DeferLoopDetected {
        depth: u8,
    },
}

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunnerError::Cwd(err) => write!(f, "failed to resolve current directory: {err}"),
            RunnerError::Resolve(err) => write!(f, "{err}"),
            RunnerError::Task(err) => write!(f, "{err}"),
            RunnerError::Ui(msg) => write!(f, "ui render failed: {msg}"),
            RunnerError::TaskInvocation(msg) => write!(f, "{msg}"),
            RunnerError::TaskCatalogsMissing { root } => write!(
                f,
                "no task catalogs found under {} (expected one or more {} files)",
                root.display(),
                TASK_MANIFEST_FILE
            ),
            RunnerError::TaskCatalogReadDir { path, error } => {
                write!(f, "failed to read directory {}: {error}", path.display())
            }
            RunnerError::TaskManifestRead { path, error } => {
                write!(f, "failed to read {}: {error}", path.display())
            }
            RunnerError::TaskManifestParse { path, error } => {
                write!(f, "failed to parse {}: {error}", path.display())
            }
            RunnerError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path,
            } => write!(
                f,
                "duplicate task catalog alias `{alias}` found in {} and {}",
                first_path.display(),
                second_path.display()
            ),
            RunnerError::TaskCatalogPrefixNotFound { prefix, available } => write!(
                f,
                "task catalog prefix `{prefix}` not found (available: {})",
                available.join(", ")
            ),
            RunnerError::TaskNotFound { name, path } => {
                write!(f, "task `{name}` is not defined in {}", path.display())
            }
            RunnerError::TaskNotFoundAny { name, catalogs } => write!(
                f,
                "task `{name}` is not defined in discovered catalogs: {}",
                catalogs.join(", ")
            ),
            RunnerError::TaskAmbiguous { name, candidates } => write!(
                f,
                "task `{name}` is ambiguous; matched multiple catalogs: {}",
                candidates.join(", ")
            ),
            RunnerError::TaskCommandLaunch { command, error } => {
                write!(f, "failed to launch task command `{command}`: {error}")
            }
            RunnerError::TaskCommandFailure {
                command,
                code,
                stdout,
                stderr,
            } => {
                if stdout.is_empty() && stderr.is_empty() {
                    write!(f, "task command failed `{command}` (code={:?})", code)
                } else {
                    write!(
                        f,
                        "task command failed `{command}` (code={:?})\nstdout:\n{}\nstderr:\n{}",
                        code, stdout, stderr
                    )
                }
            }
            RunnerError::ManagedProcess(error) => write!(f, "{error}"),
            RunnerError::TaskManagedUnsupportedMode { task, mode } => write!(
                f,
                "task `{task}` declares unsupported managed mode `{mode}` (expected `tui`)"
            ),
            RunnerError::TaskManagedProfileNotFound {
                task,
                profile,
                available,
            } => write!(
                f,
                "managed task `{task}` profile `{profile}` not found (available: {})",
                available.join(", ")
            ),
            RunnerError::TaskManagedProfileEmpty { task, profile } => write!(
                f,
                "managed task `{task}` profile `{profile}` has no processes configured"
            ),
            RunnerError::TaskManagedProcessNotFound {
                task,
                profile,
                process,
            } => write!(
                f,
                "managed task `{task}` profile `{profile}` references undefined process `{process}`"
            ),
            RunnerError::TaskManagedProcessInvalidDefinition {
                task,
                process,
                detail,
            } => write!(
                f,
                "managed task `{task}` process `{process}` is invalid: {detail}"
            ),
            RunnerError::TaskManagedProfileTabOrderInvalid {
                task,
                profile,
                detail,
            } => write!(
                f,
                "managed task `{task}` profile `{profile}` tab order is invalid: {detail}"
            ),
            RunnerError::TaskManagedTaskReferenceInvalid {
                task,
                process,
                reference,
                detail,
            } => write!(
                f,
                "managed task `{task}` process `{process}` task ref `{reference}` is invalid: {detail}"
            ),
            RunnerError::TaskManagedNonZeroExit {
                task,
                profile,
                processes,
            } => write!(
                f,
                "managed task `{task}` profile `{profile}` had non-zero exits: {}",
                processes
                    .iter()
                    .map(|(name, diagnostic)| format!("{name} ({diagnostic})"))
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            RunnerError::TaskMissingRunCommand { task, path } => write!(
                f,
                "task `{task}` in {} is missing `run` command (required for non-managed tasks)",
                path.display()
            ),
            RunnerError::BuiltinTestNonZero { failures, .. } => {
                let summary = failures
                    .iter()
                    .map(|(target, code)| match code {
                        Some(value) => format!("{target}: exit={value}"),
                        None => format!("{target}: terminated"),
                    })
                    .collect::<Vec<String>>()
                    .join(", ");
                write!(f, "one or more built-in test targets failed: {summary}")
            }
            RunnerError::DeferLoopDetected { depth } => write!(
                f,
                "deferral loop detected ({} deferral hop(s)); refusing to defer again",
                depth
            ),
        }
    }
}

impl std::error::Error for RunnerError {}

impl RunnerError {
    pub fn rendered_output(&self) -> Option<&str> {
        match self {
            RunnerError::BuiltinTestNonZero { rendered, .. } if !rendered.trim().is_empty() => {
                Some(rendered.as_str())
            }
            _ => None,
        }
    }
}

impl From<TaskError> for RunnerError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<crate::ui::UiError> for RunnerError {
    fn from(value: crate::ui::UiError) -> Self {
        Self::Ui(value.to_string())
    }
}

impl From<ResolveError> for RunnerError {
    fn from(value: ResolveError) -> Self {
        Self::Resolve(value)
    }
}

impl From<ProcessManagerError> for RunnerError {
    fn from(value: ProcessManagerError) -> Self {
        Self::ManagedProcess(value)
    }
}

#[derive(Debug, serde::Deserialize)]
struct TaskManifest {
    #[serde(default)]
    catalog: Option<ManifestCatalog>,
    #[serde(default)]
    defer: Option<ManifestDefer>,
    #[serde(default)]
    builtin: Option<ManifestBuiltin>,
    #[serde(default)]
    shell: Option<ManifestShellConfig>,
    #[serde(default, deserialize_with = "deserialize_tasks")]
    tasks: BTreeMap<String, ManifestTask>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestShellConfig {
    #[serde(default)]
    run: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestBuiltin {
    #[serde(default)]
    test: Option<ManifestBuiltinTest>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestBuiltinTest {
    #[serde(default)]
    max_parallel: Option<usize>,
    #[serde(default)]
    package_manager: Option<ManifestJsPackageManager>,
}

#[derive(Debug, Clone, Copy, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ManifestJsPackageManager {
    Bun,
    Pnpm,
    Npm,
    Direct,
}

#[derive(Debug, serde::Deserialize, Default)]
struct ManifestTask {
    #[serde(default)]
    run: Option<ManifestManagedRun>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    fail_on_non_zero: Option<bool>,
    #[serde(default)]
    shell: Option<bool>,
    #[serde(default)]
    processes: BTreeMap<String, ManifestManagedProcess>,
    #[serde(default)]
    profiles: BTreeMap<String, ManifestManagedProfile>,
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
struct ManifestManagedProcess {
    #[serde(default)]
    run: Option<ManifestManagedRun>,
    #[serde(default)]
    task: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestManagedRun {
    Command(String),
    Sequence(Vec<ManifestManagedRunStep>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestManagedRunStep {
    Command(String),
    Step(ManifestManagedRunStepTable),
}

#[derive(Debug, serde::Deserialize)]
struct ManifestManagedRunStepTable {
    #[serde(default)]
    run: Option<String>,
    #[serde(default)]
    task: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestManagedProfile {
    Table(ManifestManagedProfileTable),
    List(Vec<String>),
    Ranked(BTreeMap<String, ManifestManagedProfileOrder>),
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestManagedProfileTable {
    #[serde(default)]
    processes: Vec<String>,
    #[serde(default)]
    start: Vec<String>,
    #[serde(default)]
    tabs: Option<ManifestManagedTabOrder>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestManagedProfileOrder {
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
enum ManifestManagedTabOrder {
    List(Vec<String>),
    Ranked(BTreeMap<String, usize>),
}

impl ManifestManagedProfile {
    fn start_entries(&self) -> Vec<String> {
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

    fn tab_entries(&self) -> Option<Vec<String>> {
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

    fn start_delay_ms(&self) -> HashMap<String, u64> {
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
struct ManifestCatalog {
    alias: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestDefer {
    run: String,
}

#[derive(Debug)]
struct LoadedCatalog {
    alias: String,
    catalog_root: PathBuf,
    manifest_path: PathBuf,
    manifest: TaskManifest,
    defer_run: Option<String>,
    depth: usize,
}

#[derive(Debug)]
struct TaskSelector {
    prefix: Option<String>,
    task_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CatalogSelectionMode {
    ExplicitPrefix,
    CwdNearest,
    RootShallowest,
}

#[derive(Debug)]
struct TaskSelection<'a> {
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
    mode: CatalogSelectionMode,
    evidence: Vec<String>,
}

#[derive(Debug, Clone)]
struct DeferredCommand {
    template: String,
    working_dir: PathBuf,
    source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskRuntimeArgs {
    repo_override: Option<PathBuf>,
    verbose_root: bool,
    passthrough: Vec<String>,
}

#[derive(Debug)]
struct ManagedProcessSpec {
    name: String,
    run: String,
    cwd: PathBuf,
    start_after_ms: u64,
}

#[derive(Debug)]
struct ManagedTaskPlan {
    mode: String,
    profile: String,
    processes: Vec<ManagedProcessSpec>,
    tab_order: Vec<String>,
    fail_on_non_zero: bool,
    passthrough: Vec<String>,
}

const TASK_MANIFEST_FILE: &str = "effigy.toml";
const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";
const IMPLICIT_ROOT_DEFER_TEMPLATE: &str = "composer global exec effigy -- {request} {args}";
const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize = 3;
const DEFAULT_MANAGED_SHELL_RUN: &str = "exec ${SHELL:-/bin/zsh} -i";
const BUILTIN_TASKS: [(&str, &str); 5] = [
    ("help", "Show general help (same as --help)"),
    (
        "health",
        "Built-in health alias; falls back to repo-pulse when no explicit health task exists",
    ),
    (
        "repo-pulse",
        "Built-in repository/workspace health and structure signal report",
    ),
    (
        "test",
        "Built-in test runner detection (vitest, cargo nextest, cargo test), supports <catalog>/test fallback, optional --plan",
    ),
    ("tasks", "List discovered catalogs and available tasks"),
];

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Help(_) => Ok(String::new()),
        Command::RepoPulse(args) => run_pulse(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}

pub fn resolve_command_root(cmd: &Command) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_override = match cmd {
        Command::RepoPulse(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::Task(task) => parse_task_runtime_args(&task.args)
            .ok()
            .and_then(|parsed| parsed.repo_override),
        Command::Help(_) => None,
    };

    match resolve_target_root(cwd.clone(), repo_override) {
        Ok(resolved) => resolved.resolved_root,
        Err(_) => cwd,
    }
}

pub fn run_pulse(args: PulseArgs) -> Result<String, RunnerError> {
    let PulseArgs {
        repo_override,
        verbose_root,
    } = args;
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd.clone(), repo_override)?;

    let task = PulseTask::default();
    let ctx = TaskContext {
        target_repo: resolved.resolved_root.clone(),
        cwd,
        resolution_mode: resolved.resolution_mode,
        resolution_evidence: resolved.evidence.clone(),
        resolution_warnings: resolved.warnings.clone(),
    };

    let collected = task.collect(&ctx)?;
    let evaluated = task.evaluate(collected)?;
    let report = render_pulse_report(
        evaluated,
        verbose_root.then_some(&resolved),
        verbose_root.then_some(&ctx),
    )?;

    Ok(report)
}

pub fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd, args.repo_override)?;
    let catalogs = match discover_catalogs(&resolved.resolved_root) {
        Ok(catalogs) => catalogs,
        Err(RunnerError::TaskCatalogsMissing { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Task Catalogs")?;
    renderer.key_values(&[KeyValue::new("catalogs", catalogs.len().to_string())])?;
    renderer.text("")?;

    if let Some(filter) = args.task_name {
        let selector = parse_task_selector(&filter)?;
        renderer.section(&format!("Task Matches: {filter}"))?;
        renderer.text("")?;

        let matches = catalogs
            .iter()
            .filter_map(|catalog| {
                let task = catalog.manifest.tasks.get(&selector.task_name)?;
                if selector
                    .prefix
                    .as_ref()
                    .is_some_and(|prefix| prefix != &catalog.alias)
                {
                    return None;
                }
                Some((catalog, task))
            })
            .collect::<Vec<(&LoadedCatalog, &ManifestTask)>>();
        let builtin_matches = BUILTIN_TASKS
            .iter()
            .filter(|(name, _)| selector.prefix.is_none() && selector.task_name == *name)
            .collect::<Vec<&(&str, &str)>>();

        if matches.is_empty() && builtin_matches.is_empty() {
            renderer.notice(NoticeLevel::Warning, "no matches")?;
            let out = renderer.into_inner();
            return String::from_utf8(out).map_err(|error| {
                RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
            });
        }

        let mut rows: Vec<Vec<String>> = Vec::new();
        for (catalog, task) in matches {
            rows.push(vec![
                catalog_task_label(catalog, &selector.task_name),
                task_run_preview(task),
                catalog.manifest_path.display().to_string(),
            ]);
        }

        renderer.table(&TableSpec::new(Vec::new(), rows))?;
        renderer.text("")?;
        if !builtin_matches.is_empty() {
            let builtin_rows = builtin_matches
                .into_iter()
                .map(|(name, description)| vec![(*name).to_owned(), (*description).to_owned()])
                .collect::<Vec<Vec<String>>>();
            renderer.section("Built-in Task Matches")?;
            renderer.table(&TableSpec::new(Vec::new(), builtin_rows))?;
            renderer.text("")?;
            if selector.task_name == "test" {
                renderer.notice(
                    NoticeLevel::Info,
                    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined",
                )?;
                renderer.text("")?;
            }
        }
        renderer.summary(SummaryCounts {
            ok: 1,
            warn: 0,
            err: 0,
        })?;
        let out = renderer.into_inner();
        return String::from_utf8(out).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        });
    }

    if !catalogs.is_empty() {
        let mut ordered = catalogs.iter().collect::<Vec<&LoadedCatalog>>();
        ordered.sort_by(|a, b| {
            a.depth
                .cmp(&b.depth)
                .then_with(|| a.alias.cmp(&b.alias))
                .then_with(|| a.manifest_path.cmp(&b.manifest_path))
        });

        let mut rows: Vec<Vec<String>> = Vec::new();
        for catalog in ordered {
            if catalog.manifest.tasks.is_empty() {
                rows.push(vec![
                    "<none>".to_owned(),
                    "<none>".to_owned(),
                    catalog.manifest_path.display().to_string(),
                ]);
                continue;
            }
            for (task_name, task_def) in &catalog.manifest.tasks {
                rows.push(vec![
                    catalog_task_label(catalog, task_name),
                    task_run_preview(task_def),
                    catalog.manifest_path.display().to_string(),
                ]);
            }
        }
        renderer.table(&TableSpec::new(Vec::new(), rows))?;
        renderer.text("")?;
    }

    renderer.section("Built-in Tasks")?;
    let builtin_rows = BUILTIN_TASKS
        .iter()
        .map(|(name, description)| vec![(*name).to_owned(), (*description).to_owned()])
        .collect::<Vec<Vec<String>>>();
    renderer.table(&TableSpec::new(Vec::new(), builtin_rows))?;
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 0,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn task_run_preview(task: &ManifestTask) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

fn run_manifest_task(task: &TaskInvocation) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    run_manifest_task_with_cwd(task, cwd)
}

fn run_manifest_task_with_cwd(task: &TaskInvocation, cwd: PathBuf) -> Result<String, RunnerError> {
    let invocation_cwd = fs::canonicalize(&cwd).unwrap_or_else(|_| cwd.clone());
    let runtime_args = parse_task_runtime_args(&task.args)?;
    let resolved = resolve_target_root(cwd, runtime_args.repo_override.clone())?;
    let selector = parse_task_selector(&task.name)?;
    let catalogs = match discover_catalogs(&resolved.resolved_root) {
        Ok(catalogs) => catalogs,
        Err(RunnerError::TaskCatalogsMissing { .. }) => Vec::new(),
        Err(error) => return Err(error),
    };
    let selection = match select_catalog_and_task(&selector, &catalogs, &invocation_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if let Some(output) = try_run_builtin_task(
                &selector,
                task,
                &runtime_args,
                &resolved.resolved_root,
                &catalogs,
            )? {
                return Ok(output);
            }
            if should_attempt_deferral(&error) {
                if let Some(deferral) = select_deferral(
                    &selector,
                    &catalogs,
                    &invocation_cwd,
                    &resolved.resolved_root,
                ) {
                    return run_deferred_request(task, &runtime_args, &deferral, &error);
                }
            }
            return Err(error);
        }
    };

    let repo_for_task = selection.catalog.catalog_root.clone();
    if let Some(plan) = resolve_managed_task_plan(
        &selector,
        selection.catalog,
        selection.task,
        &runtime_args,
        &catalogs,
        &selection.catalog.catalog_root,
    )? {
        return run_or_render_managed_task(
            &selector.task_name,
            &repo_for_task,
            &selection.catalog.manifest_path,
            plan,
        );
    }

    let args_rendered = runtime_args
        .passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let run_spec =
        selection
            .task
            .run
            .as_ref()
            .ok_or_else(|| RunnerError::TaskMissingRunCommand {
                task: selector.task_name.clone(),
                path: selection.catalog.manifest_path.clone(),
            })?;
    let command = render_task_run_spec(
        &selector.task_name,
        run_spec,
        &args_rendered,
        &selection.catalog.catalog_root,
        &catalogs,
        &selection.catalog.catalog_root,
        0,
    )?;

    let mut process = ProcessCommand::new("sh");
    process.arg("-lc").arg(&command).current_dir(&repo_for_task);
    with_local_node_bin_path(&mut process, &repo_for_task);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            let trace = render_task_resolution_trace(
                &resolved,
                &selector,
                &selection,
                &repo_for_task,
                &command,
            );
            return Ok(trace);
        }
        return Ok(String::new());
    }

    Err(RunnerError::TaskCommandFailure {
        command,
        code: status.code(),
        stdout: String::new(),
        stderr: String::new(),
    })
}

fn normalize_builtin_test_suite(raw: &str) -> Option<&'static str> {
    match raw {
        "vitest" => Some("vitest"),
        "nextest" | "cargo-nextest" => Some("cargo-nextest"),
        "cargo-test" => Some("cargo-test"),
        _ => None,
    }
}

#[cfg(test)]
#[cfg(test)]
fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    builtin::builtin_test_max_parallel(catalogs, resolved_root)
}

fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    let mut repo: Option<PathBuf> = None;
    let mut verbose_root = false;
    let mut passthrough: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--repo" {
            let Some(value) = args.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "task argument --repo requires a value".to_owned(),
                ));
            };
            repo = Some(PathBuf::from(value));
            i += 2;
            continue;
        }
        if arg == "--verbose-root" {
            verbose_root = true;
            i += 1;
            continue;
        }
        passthrough.push(arg.clone());
        i += 1;
    }
    Ok(TaskRuntimeArgs {
        repo_override: repo,
        verbose_root,
        passthrough,
    })
}

fn parse_task_selector(raw: &str) -> Result<TaskSelector, RunnerError> {
    if let Some((prefix, task_name)) = raw.split_once('/') {
        if prefix.trim().is_empty() || task_name.trim().is_empty() {
            return Err(RunnerError::TaskInvocation(
                "task name must be `<task>` or `<catalog>/<task>`".to_owned(),
            ));
        }
        return Ok(TaskSelector {
            prefix: Some(prefix.trim().to_owned()),
            task_name: task_name.trim().to_owned(),
        });
    }

    if raw.trim().is_empty() {
        return Err(RunnerError::TaskInvocation(
            "task name is required".to_owned(),
        ));
    }

    Ok(TaskSelector {
        prefix: None,
        task_name: raw.trim().to_owned(),
    })
}

fn with_local_node_bin_path(process: &mut ProcessCommand, cwd: &Path) {
    let local_bin = cwd.join("node_modules/.bin");
    if !local_bin.is_dir() {
        return;
    }
    let local_rendered = local_bin.display().to_string();
    let merged = match std::env::var("PATH") {
        Ok(path) if !path.is_empty() => format!("{local_rendered}:{path}"),
        _ => local_rendered,
    };
    process.env("PATH", merged);
}

fn render_pulse_report(
    report: crate::tasks::PulseReport,
    resolved: Option<&crate::resolver::ResolvedTarget>,
    ctx: Option<&TaskContext>,
) -> Result<String, RunnerError> {
    let crate::tasks::PulseReport {
        repo: _,
        evidence,
        risk,
        next_action,
        owner,
        eta,
    } = report;
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let theme = Theme::default();
    let inline_code_on = format!("{}", theme.inline_code.render());
    let inline_code_reset = format!("{}", theme.inline_code.render_reset());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    if let (Some(resolved), Some(ctx)) = (resolved, ctx) {
        renderer.section("Root Resolution")?;
        renderer.key_values(&[
            KeyValue::new(
                "resolved-root",
                resolved.resolved_root.display().to_string(),
            ),
            KeyValue::new("mode", format!("{:?}", resolved.resolution_mode)),
        ])?;
        renderer.text("")?;
        renderer.bullet_list("evidence", &ctx.resolution_evidence)?;
        renderer.text("")?;
        renderer.bullet_list("warnings", &ctx.resolution_warnings)?;
        renderer.text("")?;
    }

    renderer.section("Pulse Report")?;
    renderer.key_values(&[
        KeyValue::new("owner", owner),
        KeyValue::new("eta", eta),
        KeyValue::new("signals", evidence.len().to_string()),
        KeyValue::new("risks", risk.len().to_string()),
        KeyValue::new("actions", next_action.len().to_string()),
    ])?;
    renderer.text("")?;
    if risk.is_empty() {
        renderer.notice(NoticeLevel::Success, "No high-priority risks detected.")?;
    } else {
        renderer.notice(
            NoticeLevel::Warning,
            &format!("Detected {} risk item(s).", risk.len()),
        )?;
    }
    renderer.text("")?;

    renderer.section("Signals")?;
    for item in &evidence {
        let styled =
            colorize_inline_code_segments(item, color_enabled, &inline_code_on, &inline_code_reset);
        renderer.text(&format!("- {styled}"))?;
    }
    renderer.text("")?;

    renderer.section("Risks")?;
    if risk.is_empty() {
        renderer.notice(NoticeLevel::Success, "No risk items.")?;
    } else {
        for item in &risk {
            let styled = colorize_inline_code_segments(
                item,
                color_enabled,
                &inline_code_on,
                &inline_code_reset,
            );
            renderer.text(&format!("- {styled}"))?;
        }
    }
    renderer.text("")?;

    renderer.section("Actions")?;
    for item in &next_action {
        let styled =
            colorize_inline_code_segments(item, color_enabled, &inline_code_on, &inline_code_reset);
        renderer.text(&format!("- {styled}"))?;
    }
    renderer.text("")?;

    renderer.summary(SummaryCounts {
        ok: evidence.len(),
        warn: risk.len(),
        err: 0,
    })?;

    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn colorize_inline_code_segments(text: &str, enabled: bool, on: &str, reset: &str) -> String {
    if !enabled || !text.contains('`') {
        return text.to_owned();
    }

    let mut out = String::new();
    let mut remaining = text;
    loop {
        let Some(start_idx) = remaining.find('`') else {
            out.push_str(remaining);
            break;
        };

        out.push_str(&remaining[..start_idx]);
        let after_start = &remaining[start_idx + 1..];
        let Some(end_idx) = after_start.find('`') else {
            out.push('`');
            out.push_str(after_start);
            break;
        };

        let code = &after_start[..end_idx];
        out.push_str(on);
        out.push('`');
        out.push_str(code);
        out.push('`');
        out.push_str(reset);

        remaining = &after_start[end_idx + 1..];
    }

    out
}

fn render_task_resolution_trace(
    resolved: &crate::resolver::ResolvedTarget,
    selector: &TaskSelector,
    selection: &TaskSelection<'_>,
    execution_cwd: &Path,
    command: &str,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Resolution");
    let mut values = vec![
        KeyValue::new("task", selector.task_name.clone()),
        KeyValue::new(
            "resolved-root",
            resolved.resolved_root.display().to_string(),
        ),
        KeyValue::new("root-mode", format!("{:?}", resolved.resolution_mode)),
        KeyValue::new("catalog-alias", selection.catalog.alias.clone()),
        KeyValue::new(
            "catalog-path",
            selection.catalog.manifest_path.display().to_string(),
        ),
        KeyValue::new("catalog-mode", format!("{:?}", selection.mode)),
        KeyValue::new("execution-cwd", execution_cwd.display().to_string()),
        KeyValue::new("command", command.to_owned()),
    ];
    if let Some(prefix) = &selector.prefix {
        values.insert(1, KeyValue::new("prefix", prefix.clone()));
    }
    let _ = renderer.key_values(&values);
    if !resolved.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-evidence", &resolved.evidence);
    }
    if !resolved.warnings.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("root-warnings", &resolved.warnings);
    }
    if !selection.evidence.is_empty() {
        let _ = renderer.text("");
        let _ = renderer.bullet_list("catalog-evidence", &selection.evidence);
    }
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}

pub(super) fn trace_renderer() -> PlainRenderer<Vec<u8>> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    PlainRenderer::new(Vec::<u8>::new(), color_enabled)
}

fn shell_quote(raw: &str) -> String {
    if raw.is_empty() {
        return "''".to_owned();
    }
    let escaped = raw.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;
