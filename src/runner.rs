use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::dev_tui::run_dev_process_tui;
use crate::process_manager::{
    ProcessEventKind, ProcessManagerError, ProcessSpec, ProcessSupervisor,
};
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::pulse::PulseTask;
use crate::tasks::{Task, TaskContext, TaskError};
use crate::testing::detect_test_runner_detailed;
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
use crate::{render_help, Command, HelpTopic, PulseArgs, TaskInvocation, TasksArgs};

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
    #[serde(default, deserialize_with = "deserialize_tasks")]
    tasks: BTreeMap<String, ManifestTask>,
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
    processes: BTreeMap<String, ManifestManagedProcess>,
    #[serde(default)]
    profiles: BTreeMap<String, ManifestManagedProfile>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum ManifestTaskDefinition {
    Run(String),
    Full(ManifestTask),
}

impl ManifestTaskDefinition {
    fn into_manifest_task(self) -> ManifestTask {
        match self {
            ManifestTaskDefinition::Run(command) => ManifestTask {
                run: Some(ManifestManagedRun::Command(command)),
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
const BUILTIN_TASKS: [(&str, &str); 4] = [
    ("help", "Show general help (same as --help)"),
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

fn is_builtin_task(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name)
}

fn resolve_builtin_task_target_root(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Option<PathBuf> {
    if let Some(prefix) = selector.prefix.as_ref() {
        return catalogs
            .iter()
            .find(|catalog| &catalog.alias == prefix)
            .map(|catalog| catalog.catalog_root.clone());
    }
    Some(resolved_root.to_path_buf())
}

fn try_run_builtin_task(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    if !is_builtin_task(&selector.task_name) {
        return Ok(None);
    }

    let Some(target_root) = resolve_builtin_task_target_root(selector, resolved_root, catalogs)
    else {
        return Ok(None);
    };

    match selector.task_name.as_str() {
        "repo-pulse" => run_builtin_repo_pulse(task, runtime_args, &target_root).map(Some),
        "tasks" => run_builtin_tasks(task, runtime_args, &target_root).map(Some),
        "help" => run_builtin_help(),
        "test" => try_run_builtin_test(selector, task, runtime_args, &target_root, catalogs),
        _ => Ok(None),
    }
}

fn run_builtin_repo_pulse(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if !runtime_args.passthrough.is_empty() {
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }
    run_pulse(PulseArgs {
        repo_override: Some(target_root.to_path_buf()),
        verbose_root: runtime_args.verbose_root,
    })
}

fn run_builtin_tasks(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    target_root: &Path,
) -> Result<String, RunnerError> {
    if runtime_args.verbose_root {
        return Err(RunnerError::TaskInvocation(format!(
            "`--verbose-root` is not supported for built-in `{}`",
            task.name
        )));
    }

    let mut task_name: Option<String> = None;
    let mut i = 0usize;
    while i < runtime_args.passthrough.len() {
        let arg = &runtime_args.passthrough[i];
        if arg == "--task" {
            let Some(value) = runtime_args.passthrough.get(i + 1) else {
                return Err(RunnerError::TaskInvocation(
                    "task argument --task requires a value".to_owned(),
                ));
            };
            task_name = Some(value.clone());
            i += 2;
            continue;
        }
        return Err(RunnerError::TaskInvocation(format!(
            "unknown argument(s) for built-in `{}`: {}",
            task.name,
            runtime_args.passthrough.join(" ")
        )));
    }

    run_tasks(TasksArgs {
        repo_override: Some(target_root.to_path_buf()),
        task_name,
    })
}

fn run_builtin_help() -> Result<Option<String>, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    render_help(&mut renderer, HelpTopic::General)?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map(Some)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn try_run_builtin_test(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Result<Option<String>, RunnerError> {
    let (flags, passthrough) = extract_builtin_test_flags(&runtime_args.passthrough);
    let targets = resolve_builtin_test_targets(selector, resolved_root, catalogs);
    let runnable = targets
        .iter()
        .filter_map(|target| {
            target
                .detection
                .selected
                .as_ref()
                .map(|plan| (target.name.clone(), target.root.clone(), plan.clone()))
        })
        .collect::<Vec<(String, PathBuf, crate::testing::TestRunnerPlan)>>();
    if runnable.is_empty() {
        return Ok(None);
    }

    if flags.plan_mode {
        let color_enabled =
            resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
        let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
        renderer.section("Test Plan")?;
        renderer.key_values(&[
            KeyValue::new("request", task.name.clone()),
            KeyValue::new("root", resolved_root.display().to_string()),
            KeyValue::new("targets", runnable.len().to_string()),
        ])?;
        renderer.text("")?;
        for target in &targets {
            let selected_runner = target.detection.selected.as_ref().map(|plan| plan.runner);
            renderer.section(&format!("Target: {}", target.name))?;
            if let Some(plan) = target.detection.selected.as_ref() {
                let args_rendered = passthrough
                    .iter()
                    .map(|arg| shell_quote(arg))
                    .collect::<Vec<String>>()
                    .join(" ");
                let command = if args_rendered.is_empty() {
                    plan.command.clone()
                } else {
                    format!("{} {}", plan.command, args_rendered)
                };
                renderer.key_values(&[
                    KeyValue::new("root", target.root.display().to_string()),
                    KeyValue::new("runner", plan.runner.label().to_owned()),
                    KeyValue::new("command", command),
                ])?;
                renderer.text("")?;
                renderer.bullet_list("evidence", &plan.evidence)?;
            } else {
                renderer.key_values(&[
                    KeyValue::new("root", target.root.display().to_string()),
                    KeyValue::new("runner", "<none>".to_owned()),
                    KeyValue::new("command", "<none>".to_owned()),
                ])?;
                renderer.text("")?;
                renderer.notice(
                    NoticeLevel::Warning,
                    "no supported test runner detected for this target",
                )?;
            }
            renderer.text("")?;
            let candidate_lines = target
                .detection
                .candidates
                .iter()
                .map(|candidate| {
                    let state = if candidate.available {
                        if Some(candidate.runner) == selected_runner {
                            "selected"
                        } else {
                            "available"
                        }
                    } else {
                        "rejected"
                    };
                    format!(
                        "{} -> {} ({state}): {}",
                        candidate.runner.label(),
                        candidate.command,
                        candidate.reason
                    )
                })
                .collect::<Vec<String>>();
            renderer.bullet_list("fallback-chain", &candidate_lines)?;
            renderer.text("")?;
        }
        let out = renderer.into_inner();
        return String::from_utf8(out).map(Some).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        });
    }

    let args_rendered = passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let max_parallel = builtin_test_max_parallel(catalogs, resolved_root);
    let results = run_builtin_test_targets_parallel(runnable, &args_rendered, max_parallel)?;
    let mut failures = results
        .iter()
        .filter_map(|result| {
            if result.success {
                None
            } else {
                Some((result.name.clone(), result.code))
            }
        })
        .collect::<Vec<(String, Option<i32>)>>();
    failures.sort_by(|a, b| a.0.cmp(&b.0));
    let rendered = render_builtin_test_results(&results, flags.verbose_results)?;
    if failures.is_empty() {
        Ok(Some(rendered))
    } else {
        Err(RunnerError::BuiltinTestNonZero { failures, rendered })
    }
}

fn extract_builtin_test_flags(raw_args: &[String]) -> (BuiltinTestCliFlags, Vec<String>) {
    let mut flags = BuiltinTestCliFlags {
        plan_mode: false,
        verbose_results: false,
    };
    let passthrough = raw_args
        .iter()
        .filter_map(|arg| {
            if arg == "--plan" {
                flags.plan_mode = true;
                None
            } else if arg == "--verbose-results" {
                flags.verbose_results = true;
                None
            } else {
                Some(arg.clone())
            }
        })
        .collect::<Vec<String>>();
    (flags, passthrough)
}

#[derive(Debug, Clone)]
struct BuiltinTestTarget {
    name: String,
    root: PathBuf,
    detection: crate::testing::TestRunnerDetection,
}

fn resolve_builtin_test_targets(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Vec<BuiltinTestTarget> {
    if let Some(prefix) = selector.prefix.as_ref() {
        if let Some(catalog) = catalogs.iter().find(|catalog| &catalog.alias == prefix) {
            return vec![BuiltinTestTarget {
                name: catalog.alias.clone(),
                detection: detect_test_runner_detailed(&catalog.catalog_root),
                root: catalog.catalog_root.clone(),
            }];
        }
        return Vec::new();
    }

    let mut targets = Vec::<BuiltinTestTarget>::new();
    let mut roots = HashMap::<PathBuf, String>::new();
    for catalog in catalogs {
        roots
            .entry(catalog.catalog_root.clone())
            .or_insert_with(|| catalog.alias.clone());
    }
    if !roots.contains_key(resolved_root) {
        roots.insert(resolved_root.to_path_buf(), "root".to_owned());
    }
    let mut ordered = roots.into_iter().collect::<Vec<(PathBuf, String)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (root, name) in ordered {
        targets.push(BuiltinTestTarget {
            name,
            detection: detect_test_runner_detailed(&root),
            root,
        });
    }
    targets
}

#[derive(Debug)]
struct BuiltinTestExecResult {
    name: String,
    runner: String,
    root: PathBuf,
    command: String,
    success: bool,
    code: Option<i32>,
}

#[derive(Debug, Clone, Copy)]
struct BuiltinTestCliFlags {
    plan_mode: bool,
    verbose_results: bool,
}

fn run_builtin_test_targets_parallel(
    runnable: Vec<(String, PathBuf, crate::testing::TestRunnerPlan)>,
    args_rendered: &str,
    max_parallel: usize,
) -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
    if runnable.is_empty() {
        return Ok(Vec::new());
    }
    let jobs = runnable
        .into_iter()
        .map(|(name, root, plan)| {
            let command = if args_rendered.is_empty() {
                plan.command
            } else {
                format!("{} {}", plan.command, args_rendered)
            };
            (name, root, plan.runner.label().to_owned(), command)
        })
        .collect::<Vec<(String, PathBuf, String, String)>>();
    let worker_count = max_parallel.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(jobs)));

    std::thread::scope(|scope| -> Result<Vec<BuiltinTestExecResult>, RunnerError> {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let queue_ref = Arc::clone(&queue);
            handles.push(scope.spawn(move || {
                let mut local = Vec::<BuiltinTestExecResult>::new();
                loop {
                    let job = {
                        let mut queue = queue_ref.lock().expect("test queue lock poisoned");
                        queue.pop_front()
                    };
                    let Some((name, root, runner, command)) = job else {
                        break;
                    };
                    let mut process = ProcessCommand::new("sh");
                    process.arg("-lc").arg(&command).current_dir(&root);
                    with_local_node_bin_path(&mut process, &root);
                    let status =
                        process
                            .status()
                            .map_err(|error| RunnerError::TaskCommandLaunch {
                                command: command.clone(),
                                error,
                            })?;
                    local.push(BuiltinTestExecResult {
                        name,
                        runner,
                        root,
                        command,
                        success: status.success(),
                        code: status.code(),
                    });
                }
                Ok::<Vec<BuiltinTestExecResult>, RunnerError>(local)
            }));
        }

        let mut combined = Vec::<BuiltinTestExecResult>::new();
        for handle in handles {
            let mut part = handle
                .join()
                .expect("builtin test worker thread panicked unexpectedly")?;
            combined.append(&mut part);
        }
        Ok(combined)
    })
}

fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    let configured = catalogs
        .iter()
        .filter(|catalog| catalog.catalog_root == resolved_root)
        .find_map(|catalog| {
            catalog
                .manifest
                .builtin
                .as_ref()
                .and_then(|builtin| builtin.test.as_ref())
                .and_then(|test| test.max_parallel)
        })
        .filter(|value| *value > 0);

    configured.unwrap_or(DEFAULT_BUILTIN_TEST_MAX_PARALLEL)
}

fn render_builtin_test_results(
    results: &[BuiltinTestExecResult],
    verbose: bool,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.text("")?;
    renderer.text("")?;
    renderer.section("Test Results")?;
    renderer.key_values(&[KeyValue::new("targets", results.len().to_string())])?;
    renderer.text("")?;
    let mut ordered = results
        .iter()
        .map(|result| {
            (
                result.name.clone(),
                result.runner.clone(),
                result.root.display().to_string(),
                result.command.clone(),
                result.success,
                result.code,
            )
        })
        .collect::<Vec<(String, String, String, String, bool, Option<i32>)>>();
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, runner, root, command, success, code) in ordered {
        let status = if success {
            "ok".to_owned()
        } else {
            match code {
                Some(value) => format!("exit={value}"),
                None => "terminated".to_owned(),
            }
        };
        let value = if verbose {
            format!("{status}  runner:{runner}  root:{root}  command:{command}")
        } else {
            status
        };
        renderer.key_values(&[KeyValue::new(name, value)])?;
    }
    renderer.text("")?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
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

fn discover_catalogs(workspace_root: &Path) -> Result<Vec<LoadedCatalog>, RunnerError> {
    let manifest_paths = discover_manifest_paths(workspace_root)?;
    if manifest_paths.is_empty() {
        return Err(RunnerError::TaskCatalogsMissing {
            root: workspace_root.to_path_buf(),
        });
    }

    let mut catalogs: Vec<LoadedCatalog> = Vec::new();
    let mut alias_map: HashMap<String, PathBuf> = HashMap::new();

    for manifest_path in manifest_paths {
        let manifest_src =
            fs::read_to_string(&manifest_path).map_err(|error| RunnerError::TaskManifestRead {
                path: manifest_path.clone(),
                error,
            })?;
        let manifest: TaskManifest =
            toml::from_str(&manifest_src).map_err(|error| RunnerError::TaskManifestParse {
                path: manifest_path.clone(),
                error,
            })?;

        let catalog_root = manifest_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| workspace_root.to_path_buf());
        let alias = manifest
            .catalog
            .as_ref()
            .and_then(|meta| meta.alias.clone())
            .unwrap_or_else(|| default_alias(&catalog_root, workspace_root));

        if let Some(first_path) = alias_map.insert(alias.clone(), manifest_path.clone()) {
            return Err(RunnerError::TaskCatalogAliasConflict {
                alias,
                first_path,
                second_path: manifest_path,
            });
        }

        catalogs.push(LoadedCatalog {
            alias,
            depth: catalog_depth(workspace_root, &catalog_root),
            catalog_root,
            manifest_path,
            defer_run: manifest.defer.as_ref().map(|defer| defer.run.clone()),
            manifest,
        });
    }

    Ok(catalogs)
}

fn should_attempt_deferral(error: &RunnerError) -> bool {
    matches!(
        error,
        RunnerError::TaskNotFoundAny { .. }
            | RunnerError::TaskCatalogPrefixNotFound { .. }
            | RunnerError::TaskNotFound { .. }
    )
}

fn select_deferral<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
    workspace_root: &Path,
) -> Option<DeferredCommand> {
    if let Some(prefix) = &selector.prefix {
        if let Some(explicit) = catalogs.iter().find(|catalog| &catalog.alias == prefix) {
            if let Some(template) = explicit.defer_run.as_ref() {
                return Some(DeferredCommand {
                    template: template.clone(),
                    working_dir: explicit.catalog_root.clone(),
                    source: format!(
                        "catalog {} ({})",
                        explicit.alias,
                        explicit.manifest_path.display()
                    ),
                });
            }
        }
    }

    let mut in_scope = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some() && cwd.starts_with(&catalog.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();
    in_scope.sort_by(|a, b| {
        b.depth
            .cmp(&a.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    if let Some(catalog) = in_scope.first() {
        if let Some(template) = catalog.defer_run.as_ref() {
            return Some(DeferredCommand {
                template: template.clone(),
                working_dir: catalog.catalog_root.clone(),
                source: format!(
                    "catalog {} ({})",
                    catalog.alias,
                    catalog.manifest_path.display()
                ),
            });
        }
    }

    let mut fallback = catalogs
        .iter()
        .filter(|catalog| catalog.defer_run.is_some())
        .collect::<Vec<&LoadedCatalog>>();
    fallback.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    if let Some(catalog) = fallback.first() {
        if let Some(template) = catalog.defer_run.as_ref() {
            return Some(DeferredCommand {
                template: template.clone(),
                working_dir: catalog.catalog_root.clone(),
                source: format!(
                    "catalog {} ({})",
                    catalog.alias,
                    catalog.manifest_path.display()
                ),
            });
        }
    }

    infer_implicit_root_deferral(workspace_root)
}

fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    deferral: &DeferredCommand,
    cause: &RunnerError,
) -> Result<String, RunnerError> {
    let current_depth = std::env::var(DEFER_DEPTH_ENV)
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(0);
    if current_depth >= 1 {
        return Err(RunnerError::DeferLoopDetected {
            depth: current_depth,
        });
    }

    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&deferral.working_dir.display().to_string());
    let command = deferral
        .template
        .replace("{request}", &request_rendered)
        .replace("{args}", &args_rendered)
        .replace("{repo}", &repo_rendered);

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "sh".to_owned());
    let shell_arg = if shell.ends_with("zsh") || shell.ends_with("bash") {
        "-ic"
    } else {
        "-lc"
    };
    let mut process = ProcessCommand::new(&shell);
    process
        .arg(shell_arg)
        .arg(&command)
        .current_dir(&deferral.working_dir)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string());
    with_local_node_bin_path(&mut process, &deferral.working_dir);
    let status = process
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, deferral, &command, cause));
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

fn infer_implicit_root_deferral(workspace_root: &Path) -> Option<DeferredCommand> {
    let has_effigy_json = workspace_root.join("effigy.json").is_file();
    let has_composer_json = workspace_root.join("composer.json").is_file();
    if has_effigy_json && has_composer_json {
        return Some(DeferredCommand {
            template: IMPLICIT_ROOT_DEFER_TEMPLATE.to_owned(),
            working_dir: workspace_root.to_path_buf(),
            source: "implicit root deferral (composer.json + effigy.json)".to_owned(),
        });
    }
    None
}

fn render_deferral_trace(
    task: &TaskInvocation,
    deferral: &DeferredCommand,
    command: &str,
    cause: &RunnerError,
) -> String {
    let mut renderer = trace_renderer();
    let _ = renderer.section("Task Deferral");
    let _ = renderer.key_values(&[
        KeyValue::new("request", task.name.clone()),
        KeyValue::new("defer-source", deferral.source.clone()),
        KeyValue::new("working-dir", deferral.working_dir.display().to_string()),
        KeyValue::new("command", command.to_owned()),
        KeyValue::new("reason", cause.to_string()),
    ]);
    let out = renderer.into_inner();
    String::from_utf8_lossy(&out).to_string()
}

fn resolve_managed_task_plan(
    selector: &TaskSelector,
    _catalog: &LoadedCatalog,
    task: &ManifestTask,
    runtime_args: &TaskRuntimeArgs,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Option<ManagedTaskPlan>, RunnerError> {
    let Some(mode) = task.mode.as_deref() else {
        return Ok(None);
    };
    if mode != "tui" {
        return Err(RunnerError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile_name = runtime_args
        .passthrough
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_owned());
    let profile = task.profiles.get(&profile_name).ok_or_else(|| {
        let mut available = task.profiles.keys().cloned().collect::<Vec<String>>();
        available.sort();
        RunnerError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name.clone(),
            available,
        }
    })?;

    let profile_entries = profile.start_entries();
    if profile_entries.is_empty() {
        return Err(RunnerError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile_name,
        });
    }

    let start_delay_ms = profile.start_delay_ms();
    let mut processes = Vec::with_capacity(profile_entries.len());
    if task.processes.is_empty() {
        for (idx, entry) in profile_entries.iter().enumerate() {
            let (name, run, cwd) = resolve_direct_profile_entry(
                &selector.task_name,
                entry,
                idx + 1,
                catalogs,
                task_scope_cwd,
            )?;
            processes.push(ManagedProcessSpec {
                name,
                run,
                cwd,
                start_after_ms: *start_delay_ms.get(entry).unwrap_or(&0),
            });
        }
    } else {
        for process_name in &profile_entries {
            let process = task.processes.get(process_name).ok_or_else(|| {
                RunnerError::TaskManagedProcessNotFound {
                    task: selector.task_name.clone(),
                    profile: profile_name.clone(),
                    process: process_name.clone(),
                }
            })?;
            let (process_run, process_cwd) = resolve_managed_process_run(
                &selector.task_name,
                process_name,
                process,
                catalogs,
                task_scope_cwd,
            )?;
            processes.push(ManagedProcessSpec {
                name: process_name.clone(),
                run: process_run,
                cwd: process_cwd,
                start_after_ms: *start_delay_ms.get(process_name).unwrap_or(&0),
            });
        }
    }

    let tab_order =
        resolve_managed_tab_order(&selector.task_name, &profile_name, &processes, profile)?;

    Ok(Some(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name,
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: runtime_args.passthrough.iter().skip(1).cloned().collect(),
    }))
}

fn resolve_managed_tab_order(
    task_name: &str,
    profile_name: &str,
    processes: &[ManagedProcessSpec],
    profile: &ManifestManagedProfile,
) -> Result<Vec<String>, RunnerError> {
    let process_names = processes
        .iter()
        .map(|process| process.name.clone())
        .collect::<Vec<String>>();
    let Some(tab_entries) = profile.tab_entries() else {
        return Ok(process_names);
    };

    let mut tab_order = Vec::with_capacity(process_names.len());
    for tab in &tab_entries {
        if !process_names.iter().any(|name| name == tab) {
            return Err(RunnerError::TaskManagedProfileTabOrderInvalid {
                task: task_name.to_owned(),
                profile: profile_name.to_owned(),
                detail: format!("tab `{tab}` is not a configured process in this profile"),
            });
        }
        if tab_order.iter().any(|name| name == tab) {
            return Err(RunnerError::TaskManagedProfileTabOrderInvalid {
                task: task_name.to_owned(),
                profile: profile_name.to_owned(),
                detail: format!("tab `{tab}` is duplicated"),
            });
        }
        tab_order.push(tab.to_owned());
    }
    for process_name in process_names {
        if !tab_order.iter().any(|name| name == &process_name) {
            tab_order.push(process_name);
        }
    }
    Ok(tab_order)
}

fn resolve_managed_process_run(
    managed_task_name: &str,
    process_name: &str,
    process: &ManifestManagedProcess,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    match (&process.run, &process.task) {
        (Some(run), None) => resolve_managed_run_value(
            managed_task_name,
            process_name,
            run,
            catalogs,
            task_scope_cwd,
        ),
        (None, Some(task_ref)) => resolve_task_reference_run(
            managed_task_name,
            process_name,
            task_ref,
            catalogs,
            task_scope_cwd,
        ),
        (Some(_), Some(_)) => Err(RunnerError::TaskManagedProcessInvalidDefinition {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            detail: "define either `run` or `task`, not both".to_owned(),
        }),
        (None, None) => Err(RunnerError::TaskManagedProcessInvalidDefinition {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            detail: "missing both `run` and `task`".to_owned(),
        }),
    }
}

fn resolve_managed_run_value(
    managed_task_name: &str,
    process_name: &str,
    run: &ManifestManagedRun,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    match run {
        ManifestManagedRun::Command(command) => Ok((command.clone(), task_scope_cwd.to_path_buf())),
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                    task: managed_task_name.to_owned(),
                    process: process_name.to_owned(),
                    detail: "run array must include at least one step".to_owned(),
                });
            }
            let mut commands = Vec::with_capacity(steps.len());
            for (idx, step) in steps.iter().enumerate() {
                let step_num = idx + 1;
                let step_command = match step {
                    ManifestManagedRunStep::Command(command) => {
                        if let Some(task_ref) = command
                            .strip_prefix("task:")
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                        {
                            let (task_run, task_cwd) = resolve_task_reference_run(
                                managed_task_name,
                                process_name,
                                task_ref,
                                catalogs,
                                task_scope_cwd,
                            )?;
                            format!(
                                "(cd {} && {})",
                                shell_quote(&task_cwd.display().to_string()),
                                task_run
                            )
                        } else {
                            command.clone()
                        }
                    }
                    ManifestManagedRunStep::Step(step) => match (&step.run, &step.task) {
                        (Some(run), None) => run.clone(),
                        (None, Some(task_ref)) => {
                            let (task_run, task_cwd) = resolve_task_reference_run(
                                managed_task_name,
                                process_name,
                                task_ref,
                                catalogs,
                                task_scope_cwd,
                            )?;
                            format!(
                                "(cd {} && {})",
                                shell_quote(&task_cwd.display().to_string()),
                                task_run
                            )
                        }
                        (Some(_), Some(_)) => {
                            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                                task: managed_task_name.to_owned(),
                                process: process_name.to_owned(),
                                detail: format!(
                                    "run step {step_num} defines both `run` and `task`; choose one"
                                ),
                            });
                        }
                        (None, None) => {
                            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                                task: managed_task_name.to_owned(),
                                process: process_name.to_owned(),
                                detail: format!(
                                    "run step {step_num} is missing both `run` and `task`"
                                ),
                            });
                        }
                    },
                };
                commands.push(step_command);
            }
            Ok((commands.join(" && "), task_scope_cwd.to_path_buf()))
        }
    }
}

fn resolve_task_reference_run(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    let selector = parse_task_selector(task_ref).map_err(|error| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: error.to_string(),
        }
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskManagedTaskReferenceInvalid {
                task: managed_task_name.to_owned(),
                process: process_name.to_owned(),
                reference: task_ref.to_owned(),
                detail: error.to_string(),
            }
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: format!(
                "referenced task `{}` in {} has no `run` command",
                selector.task_name,
                selection.catalog.manifest_path.display()
            ),
        }
    })?;
    let run_rendered = render_task_run_spec(
        &selector.task_name,
        run_spec,
        "",
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| RunnerError::TaskManagedTaskReferenceInvalid {
        task: managed_task_name.to_owned(),
        process: process_name.to_owned(),
        reference: task_ref.to_owned(),
        detail: error.to_string(),
    })?;
    Ok((run_rendered, selection.catalog.catalog_root.clone()))
}

fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if depth > 12 {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run expansion exceeded maximum nested task references (12)"
        )));
    }
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    match run {
        ManifestManagedRun::Command(command) => Ok(command
            .replace("{repo}", &repo_rendered)
            .replace("{args}", args_rendered)),
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "task `{task_name}` has an empty run array"
                )));
            }
            let mut commands = Vec::with_capacity(steps.len());
            for step in steps {
                commands.push(resolve_task_run_step(
                    task_name,
                    step,
                    args_rendered,
                    repo_root,
                    catalogs,
                    task_scope_cwd,
                    depth + 1,
                )?);
            }
            Ok(commands.join(" && "))
        }
    }
}

fn resolve_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStep,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                resolve_task_reference_step(
                    task_name,
                    task_ref,
                    args_rendered,
                    catalogs,
                    task_scope_cwd,
                    depth,
                )
            } else {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(command
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
        }
        ManifestManagedRunStep::Step(step) => match (&step.run, &step.task) {
            (Some(run), None) => {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(run
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
            (None, Some(task_ref)) => resolve_task_reference_step(
                task_name,
                task_ref,
                args_rendered,
                catalogs,
                task_scope_cwd,
                depth,
            ),
            (Some(_), Some(_)) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: define either `run` or `task`, not both"
            ))),
            (None, None) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: missing both `run` and `task`"
            ))),
        },
    }
}

fn resolve_task_reference_step(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    let selector = parse_task_selector(task_ref).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` is invalid: {error}"
        ))
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step task ref `{task_ref}` failed: {error}"
            ))
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
            selection.catalog.manifest_path.display()
        ))
    })?;
    let nested = render_task_run_spec(
        &selector.task_name,
        run_spec,
        args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        depth,
    )?;
    Ok(format!(
        "(cd {} && {})",
        shell_quote(&selection.catalog.catalog_root.display().to_string()),
        nested
    ))
}

fn resolve_direct_profile_entry(
    managed_task_name: &str,
    entry: &str,
    ordinal: usize,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, String, PathBuf), RunnerError> {
    let selector = parse_task_selector(entry).map_err(|error| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: format!("entry-{ordinal}"),
            reference: entry.to_owned(),
            detail: error.to_string(),
        }
    })?;
    let selection =
        select_catalog_and_task(&selector, catalogs, task_scope_cwd).map_err(|error| {
            RunnerError::TaskManagedTaskReferenceInvalid {
                task: managed_task_name.to_owned(),
                process: format!("entry-{ordinal}"),
                reference: entry.to_owned(),
                detail: error.to_string(),
            }
        })?;
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: format!("entry-{ordinal}"),
            reference: entry.to_owned(),
            detail: format!(
                "referenced task `{}` in {} has no `run` command",
                selector.task_name,
                selection.catalog.manifest_path.display()
            ),
        }
    })?;
    let run = render_task_run_spec(
        &selector.task_name,
        run_spec,
        "",
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| RunnerError::TaskManagedTaskReferenceInvalid {
        task: managed_task_name.to_owned(),
        process: format!("entry-{ordinal}"),
        reference: entry.to_owned(),
        detail: error.to_string(),
    })?;
    let name = selector
        .prefix
        .map(|prefix| format!("{prefix}/{}", selector.task_name))
        .unwrap_or(selector.task_name);
    Ok((name, run, selection.catalog.catalog_root.clone()))
}

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Plan")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile),
        KeyValue::new("repo-root", repo_root.display().to_string()),
        KeyValue::new("manifest", manifest_path.display().to_string()),
        KeyValue::new("processes", plan.processes.len().to_string()),
        KeyValue::new("tab-order", plan.tab_order.join(", ")),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Interactive TUI runtime is available for this task.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Set EFFIGY_MANAGED_STREAM=1 to run selected profile processes in stream mode.",
    )?;
    renderer.text("")?;
    let rows = plan
        .processes
        .into_iter()
        .map(|process| {
            vec![
                process.name,
                process.cwd.display().to_string(),
                process.run,
                process.start_after_ms.to_string(),
            ]
        })
        .collect::<Vec<Vec<String>>>();
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "cwd".to_owned(),
            "run".to_owned(),
            "start-after-ms".to_owned(),
        ],
        rows,
    ))?;
    if !plan.passthrough.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("profile-args", &plan.passthrough)?;
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let tui_override = std::env::var("EFFIGY_MANAGED_TUI").ok();
    let should_stream = std::env::var("EFFIGY_MANAGED_STREAM")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    if should_stream {
        return run_managed_task_runtime(task_name, repo_root, plan);
    }

    let should_tui = match tui_override.as_deref() {
        Some("1") => true,
        Some(value) if value.eq_ignore_ascii_case("true") => true,
        Some("0") => false,
        Some(value) if value.eq_ignore_ascii_case("false") => false,
        _ => std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    };
    if should_tui {
        return run_managed_task_tui(task_name, repo_root, plan);
    }

    render_managed_task_plan(task_name, repo_root, manifest_path, plan)
}

fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        ..
    } = plan;
    let specs = processes
        .into_iter()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let outcome =
        run_dev_process_tui(repo_root.to_path_buf(), specs, tab_order).map_err(|error| {
            RunnerError::Ui(format!(
                "managed tui runtime failed for task `{task_name}`: {error}"
            ))
        })?;
    if fail_on_non_zero && !outcome.non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile,
            processes: outcome.non_zero_exits,
        });
    }
    Ok(String::new())
}

fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let specs = plan
        .processes
        .iter()
        .map(|process| ProcessSpec {
            name: process.name.clone(),
            run: process.run.clone(),
            cwd: process.cwd.clone(),
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Runtime")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile.clone()),
        KeyValue::new("processes", expected.to_string()),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Running managed profile in temporary stream mode.",
    )?;
    renderer.text("")?;

    let mut exit_count = 0usize;
    let mut drained_after_exit = 0usize;
    let mut non_zero_exits = Vec::<(String, String)>::new();
    while exit_count < expected || drained_after_exit < 3 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if exit_count >= expected {
                drained_after_exit = 0;
            }
            match event.kind {
                ProcessEventKind::Stdout => {
                    renderer.text(&format!("[{}] {}", event.process, event.payload))?;
                }
                ProcessEventKind::Stderr => {
                    renderer.text(&format!("[{} stderr] {}", event.process, event.payload))?;
                }
                ProcessEventKind::Exit => {
                    exit_count += 1;
                    if event.payload != "exit=0" {
                        non_zero_exits.push((event.process.clone(), event.payload.clone()));
                    }
                    renderer.notice(
                        NoticeLevel::Info,
                        &format!("process `{}` {}", event.process, event.payload),
                    )?;
                }
            }
        } else if exit_count >= expected {
            drained_after_exit += 1;
        }
    }

    supervisor.terminate_all();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if plan.fail_on_non_zero && !non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile: plan.profile,
            processes: non_zero_exits,
        });
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: expected,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
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

fn discover_manifest_paths(workspace_root: &Path) -> Result<Vec<PathBuf>, RunnerError> {
    let mut pending: Vec<PathBuf> = vec![workspace_root.to_path_buf()];
    let mut manifests_by_catalog: HashMap<PathBuf, PathBuf> = HashMap::new();

    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir).map_err(|error| RunnerError::TaskCatalogReadDir {
            path: dir.clone(),
            error,
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| RunnerError::TaskCatalogReadDir {
                path: dir.clone(),
                error,
            })?;

            let path = entry.path();
            let file_type = entry
                .file_type()
                .map_err(|error| RunnerError::TaskCatalogReadDir {
                    path: path.clone(),
                    error,
                })?;

            if file_type.is_dir() {
                if should_skip_dir(&path) {
                    continue;
                }
                pending.push(path);
                continue;
            }

            if file_type.is_file()
                && path.file_name().and_then(|n| n.to_str()) == Some(TASK_MANIFEST_FILE)
            {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                manifests_by_catalog.insert(catalog_root, path);
                continue;
            }
        }
    }

    let mut manifests: Vec<PathBuf> = manifests_by_catalog.into_values().collect();
    manifests.sort();
    Ok(manifests)
}

fn should_skip_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(".git" | "node_modules" | "target" | ".next")
    )
}

fn select_catalog_and_task<'a>(
    selector: &TaskSelector,
    catalogs: &'a [LoadedCatalog],
    cwd: &Path,
) -> Result<TaskSelection<'a>, RunnerError> {
    if let Some(prefix) = &selector.prefix {
        let mut available = catalogs
            .iter()
            .map(|c| c.alias.clone())
            .collect::<Vec<String>>();
        available.sort();

        let Some(catalog) = catalogs.iter().find(|c| &c.alias == prefix) else {
            return Err(RunnerError::TaskCatalogPrefixNotFound {
                prefix: prefix.clone(),
                available,
            });
        };

        let Some(task) = catalog.manifest.tasks.get(&selector.task_name) else {
            return Err(RunnerError::TaskNotFound {
                name: selector.task_name.clone(),
                path: catalog.manifest_path.clone(),
            });
        };
        return Ok(TaskSelection {
            catalog,
            task,
            mode: CatalogSelectionMode::ExplicitPrefix,
            evidence: vec![format!("selected catalog via explicit prefix `{prefix}`")],
        });
    }

    let matches = catalogs
        .iter()
        .filter(|c| c.manifest.tasks.contains_key(&selector.task_name))
        .collect::<Vec<&LoadedCatalog>>();

    if matches.is_empty() {
        return Err(RunnerError::TaskNotFoundAny {
            name: selector.task_name.clone(),
            catalogs: catalogs.iter().map(format_catalog).collect(),
        });
    }

    let in_scope = matches
        .iter()
        .copied()
        .filter(|c| cwd.starts_with(&c.catalog_root))
        .collect::<Vec<&LoadedCatalog>>();

    if !in_scope.is_empty() {
        let max_depth = in_scope.iter().map(|c| c.depth).max().unwrap_or_default();
        let deepest = in_scope
            .into_iter()
            .filter(|c| c.depth == max_depth)
            .collect::<Vec<&LoadedCatalog>>();
        if deepest.len() > 1 {
            return Err(RunnerError::TaskAmbiguous {
                name: selector.task_name.clone(),
                candidates: deepest.into_iter().map(format_catalog).collect(),
            });
        }
        let selected = deepest[0];
        let evidence = vec![format!(
            "selected nearest in-scope catalog `{}` for cwd {}",
            selected.alias,
            cwd.display()
        )];
        let task = selected
            .manifest
            .tasks
            .get(&selector.task_name)
            .expect("task existence already validated");
        return Ok(TaskSelection {
            catalog: selected,
            task,
            mode: CatalogSelectionMode::CwdNearest,
            evidence,
        });
    }

    let min_depth = matches.iter().map(|c| c.depth).min().unwrap_or_default();
    let shallowest = matches
        .into_iter()
        .filter(|c| c.depth == min_depth)
        .collect::<Vec<&LoadedCatalog>>();
    if shallowest.len() > 1 {
        return Err(RunnerError::TaskAmbiguous {
            name: selector.task_name.clone(),
            candidates: shallowest.into_iter().map(format_catalog).collect(),
        });
    }
    let selected = shallowest[0];
    let evidence = vec![format!(
        "selected shallowest catalog `{}` by depth {} from workspace root",
        selected.alias, selected.depth
    )];
    let task = selected
        .manifest
        .tasks
        .get(&selector.task_name)
        .expect("task existence already validated");
    Ok(TaskSelection {
        catalog: selected,
        task,
        mode: CatalogSelectionMode::RootShallowest,
        evidence,
    })
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

fn trace_renderer() -> PlainRenderer<Vec<u8>> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    PlainRenderer::new(Vec::<u8>::new(), color_enabled)
}

fn format_catalog(catalog: &LoadedCatalog) -> String {
    format!("{} ({})", catalog.alias, catalog.manifest_path.display())
}

fn catalog_depth(workspace_root: &Path, catalog_root: &Path) -> usize {
    catalog_root
        .strip_prefix(workspace_root)
        .map(|rel| rel.components().count())
        .unwrap_or(usize::MAX)
}

fn default_alias(catalog_root: &Path, workspace_root: &Path) -> String {
    if catalog_root == workspace_root {
        return "root".to_owned();
    }

    catalog_root
        .file_name()
        .and_then(|n| n.to_str())
        .map(|v| v.to_owned())
        .unwrap_or_else(|| "catalog".to_owned())
}

fn shell_quote(raw: &str) -> String {
    if raw.is_empty() {
        return "''".to_owned();
    }
    let escaped = raw.replace('\'', "'\"'\"'");
    format!("'{escaped}'")
}

#[cfg(test)]
#[path = "tests/runner_tests.rs"]
mod tests;
