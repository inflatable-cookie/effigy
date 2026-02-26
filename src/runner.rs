use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::pulse::PulseTask;
use crate::tasks::{Task, TaskContext, TaskError};
use crate::ui::{KeyValue, NoticeLevel, PlainRenderer, Renderer, SummaryCounts, TableSpec};
use crate::{Command, PulseArgs, TaskInvocation, TasksArgs};

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
            RunnerError::DeferLoopDetected { depth } => write!(
                f,
                "deferral loop detected ({} deferral hop(s)); refusing to defer again",
                depth
            ),
        }
    }
}

impl std::error::Error for RunnerError {}

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

#[derive(Debug, serde::Deserialize)]
struct TaskManifest {
    #[serde(default)]
    catalog: Option<ManifestCatalog>,
    #[serde(default)]
    defer: Option<ManifestDefer>,
    #[serde(default)]
    tasks: BTreeMap<String, ManifestTask>,
}

#[derive(Debug, serde::Deserialize)]
struct ManifestTask {
    run: String,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct TaskRuntimeArgs {
    repo_override: Option<PathBuf>,
    verbose_root: bool,
    passthrough: Vec<String>,
}

const TASK_MANIFEST_FILE: &str = "effigy.tasks.toml";
const LEGACY_TASK_MANIFEST_FILE: &str = "underlay.tasks.toml";
const DEFER_DEPTH_ENV: &str = "EFFIGY_DEFER_DEPTH";

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Help => Ok(String::new()),
        Command::RepoPulse(args) => run_pulse(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
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
        resolution_evidence: resolved.evidence,
        resolution_warnings: resolved.warnings,
    };

    let collected = task.collect(&ctx)?;
    let evaluated = task.evaluate(collected)?;
    let report = task.render(evaluated).map_err(RunnerError::from)?;

    if verbose_root {
        let mut trace = String::new();
        trace.push_str("# Root Resolution\n\n");
        trace.push_str(&format!(
            "- resolved-root: {}\n",
            resolved.resolved_root.display()
        ));
        trace.push_str(&format!("- mode: {:?}\n", resolved.resolution_mode));
        if !ctx.resolution_evidence.is_empty() {
            trace.push_str("- evidence:\n");
            for item in &ctx.resolution_evidence {
                trace.push_str(&format!("  - {}\n", item));
            }
        }
        if !ctx.resolution_warnings.is_empty() {
            trace.push_str("- warnings:\n");
            for item in &ctx.resolution_warnings {
                trace.push_str(&format!("  - {}\n", item));
            }
        }
        trace.push('\n');
        trace.push_str(&report);
        return Ok(trace);
    }

    Ok(report)
}

pub fn run_tasks(args: TasksArgs) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    let resolved = resolve_target_root(cwd, args.repo_override)?;
    let catalogs = discover_catalogs(&resolved.resolved_root)?;
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), false);
    renderer.section("Task Catalogs")?;
    renderer.key_values(&[
        KeyValue::new("root", resolved.resolved_root.display().to_string()),
        KeyValue::new("catalogs", catalogs.len().to_string()),
    ])?;

    if let Some(filter) = args.task_name {
        let selector = parse_task_selector(&filter)?;
        renderer.section(&format!("Task Matches: {filter}"))?;

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

        if matches.is_empty() {
            renderer.notice(NoticeLevel::Warning, "no matches")?;
            let out = renderer.into_inner();
            return String::from_utf8(out).map_err(|error| {
                RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
            });
        }

        let mut rows: Vec<Vec<String>> = Vec::new();
        for (catalog, task) in matches {
            rows.push(vec![
                catalog.alias.clone(),
                selector.task_name.clone(),
                task.run.clone(),
                catalog.manifest_path.display().to_string(),
            ]);
        }

        renderer.table(&TableSpec::new(
            vec![
                "catalog".to_owned(),
                "task".to_owned(),
                "run".to_owned(),
                "manifest".to_owned(),
            ],
            rows,
        ))?;
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
                catalog.alias.clone(),
                "<none>".to_owned(),
                "<none>".to_owned(),
                catalog.manifest_path.display().to_string(),
            ]);
            continue;
        }
        for (task_name, task_def) in &catalog.manifest.tasks {
            rows.push(vec![
                catalog.alias.clone(),
                task_name.clone(),
                task_def.run.clone(),
                catalog.manifest_path.display().to_string(),
            ]);
        }
    }

    renderer.table(&TableSpec::new(
        vec![
            "catalog".to_owned(),
            "task".to_owned(),
            "run".to_owned(),
            "manifest".to_owned(),
        ],
        rows,
    ))?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 0,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
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
    let catalogs = discover_catalogs(&resolved.resolved_root)?;
    let selection = match select_catalog_and_task(&selector, &catalogs, &invocation_cwd) {
        Ok(selection) => selection,
        Err(error) if should_attempt_deferral(&error) => {
            if let Some(deferral) = select_deferral(&selector, &catalogs, &invocation_cwd) {
                return run_deferred_request(task, &runtime_args, deferral, &error);
            }
            return Err(error);
        }
        Err(error) => return Err(error),
    };

    let repo_for_task = selection.catalog.catalog_root.clone();

    let args_rendered = runtime_args
        .passthrough
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let repo_rendered = shell_quote(&repo_for_task.display().to_string());

    let command = selection
        .task
        .run
        .replace("{repo}", &repo_rendered)
        .replace("{args}", &args_rendered);

    let status = ProcessCommand::new("sh")
        .arg("-lc")
        .arg(&command)
        .current_dir(&repo_for_task)
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
    if let Some((prefix, task_name)) = raw.split_once(':') {
        if prefix.trim().is_empty() || task_name.trim().is_empty() {
            return Err(RunnerError::TaskInvocation(
                "task name must be `<task>` or `<catalog>:<task>`".to_owned(),
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
) -> Option<&'a LoadedCatalog> {
    if let Some(prefix) = &selector.prefix {
        if let Some(explicit) = catalogs.iter().find(|catalog| &catalog.alias == prefix) {
            if explicit.defer_run.is_some() {
                return Some(explicit);
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
        return Some(catalog);
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
    fallback.first().copied()
}

fn run_deferred_request(
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    catalog: &LoadedCatalog,
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

    let defer_template = catalog
        .defer_run
        .as_ref()
        .expect("deferral selected only when defer_run exists");

    let args_rendered = runtime_args.passthrough.join(" ");
    let request_rendered = task.name.clone();
    let repo_rendered = shell_quote(&catalog.catalog_root.display().to_string());
    let command = defer_template
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
    let status = ProcessCommand::new(&shell)
        .arg(shell_arg)
        .arg(&command)
        .current_dir(&catalog.catalog_root)
        .env(DEFER_DEPTH_ENV, (current_depth + 1).to_string())
        .status()
        .map_err(|error| RunnerError::TaskCommandLaunch {
            command: command.clone(),
            error,
        })?;

    if status.success() {
        if runtime_args.verbose_root {
            return Ok(render_deferral_trace(task, catalog, &command, cause));
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

fn render_deferral_trace(
    task: &TaskInvocation,
    catalog: &LoadedCatalog,
    command: &str,
    cause: &RunnerError,
) -> String {
    let mut trace = String::new();
    trace.push_str("# Task Deferral\n\n");
    trace.push_str(&format!("- request: {}\n", task.name));
    trace.push_str(&format!(
        "- defer-catalog: {} ({})\n",
        catalog.alias,
        catalog.manifest_path.display()
    ));
    trace.push_str(&format!("- command: `{}`\n", command));
    trace.push_str(&format!("- reason: {}\n", cause));
    trace
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

            if file_type.is_file()
                && path.file_name().and_then(|n| n.to_str()) == Some(LEGACY_TASK_MANIFEST_FILE)
            {
                let catalog_root = path.parent().map(Path::to_path_buf).unwrap_or_default();
                manifests_by_catalog.entry(catalog_root).or_insert(path);
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
    let mut trace = String::new();
    trace.push_str("# Task Resolution\n\n");
    trace.push_str(&format!("- task: {}\n", selector.task_name));
    if let Some(prefix) = &selector.prefix {
        trace.push_str(&format!("- prefix: {}\n", prefix));
    }
    trace.push_str(&format!(
        "- resolved-root: {}\n",
        resolved.resolved_root.display()
    ));
    trace.push_str(&format!("- root-mode: {:?}\n", resolved.resolution_mode));
    trace.push_str(&format!("- catalog-alias: {}\n", selection.catalog.alias));
    trace.push_str(&format!(
        "- catalog-path: {}\n",
        selection.catalog.manifest_path.display()
    ));
    trace.push_str(&format!("- catalog-mode: {:?}\n", selection.mode));
    trace.push_str(&format!("- execution-cwd: {}\n", execution_cwd.display()));
    trace.push_str(&format!("- command: `{}`\n", command));

    if !resolved.evidence.is_empty() {
        trace.push_str("- root-evidence:\n");
        for item in &resolved.evidence {
            trace.push_str(&format!("  - {}\n", item));
        }
    }
    if !resolved.warnings.is_empty() {
        trace.push_str("- root-warnings:\n");
        for item in &resolved.warnings {
            trace.push_str(&format!("  - {}\n", item));
        }
    }
    if !selection.evidence.is_empty() {
        trace.push_str("- catalog-evidence:\n");
        for item in &selection.evidence {
            trace.push_str(&format!("  - {}\n", item));
        }
    }
    trace
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
