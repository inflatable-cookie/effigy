use std::io::IsTerminal;
use std::path::PathBuf;

use crate::process_manager::ProcessManagerError;
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::pulse::PulseTask;
use crate::tasks::{Task, TaskContext, TaskError};
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};
#[cfg(test)]
use crate::TaskInvocation;
use crate::{Command, PulseArgs, TasksArgs};

mod builtin;
mod catalog;
mod deferral;
mod execute;
mod managed;
mod manifest;
mod model;
mod render;
mod util;

use builtin::try_run_builtin_task;
use catalog::discover_catalogs;
use execute::{catalog_task_label, run_manifest_task, task_run_preview};
use manifest::{
    ManifestJsPackageManager, ManifestManagedProcess, ManifestManagedProfile, ManifestManagedRun,
    ManifestManagedRunStep, ManifestTask, TaskManifest,
};
use model::{
    CatalogSelectionMode, DeferredCommand, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    TaskRuntimeArgs, TaskSelection, TaskSelector, BUILTIN_TASKS, DEFAULT_BUILTIN_TEST_MAX_PARALLEL,
    DEFAULT_MANAGED_SHELL_RUN, DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE, TASK_MANIFEST_FILE,
};
use render::render_pulse_report;
use util::{parse_task_runtime_args, parse_task_selector};

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

#[cfg(test)]
fn run_manifest_task_with_cwd(task: &TaskInvocation, cwd: PathBuf) -> Result<String, RunnerError> {
    execute::run_manifest_task_with_cwd(task, cwd)
}

#[cfg(test)]
fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &std::path::Path) -> usize {
    builtin::builtin_test_max_parallel(catalogs, resolved_root)
}

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;
