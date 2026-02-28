use std::io::IsTerminal;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::process_manager::ProcessManagerError;
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::pulse::PulseTask;
use crate::tasks::{Task, TaskContext, TaskError};
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
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
use catalog::{discover_catalogs, select_catalog_and_task};
use execute::{catalog_task_label, run_manifest_task, task_run_preview};
use manifest::{
    ManifestJsPackageManager, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestManagedRunStep, ManifestTask, TaskManifest,
};
use model::{
    CatalogSelectionMode, DeferredCommand, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    TaskRuntimeArgs, TaskSelection, TaskSelector, BUILTIN_TASKS, DEFAULT_BUILTIN_TEST_MAX_PARALLEL,
    DEFAULT_MANAGED_SHELL_RUN, DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE, TASK_MANIFEST_FILE,
};
use render::render_pulse_report;
use util::{parse_task_reference_invocation, parse_task_runtime_args, parse_task_selector};

#[derive(Debug)]
struct ManagedProfileDisplayRow {
    task: String,
    run: String,
    profile: String,
    invocation: String,
    parent_task: String,
}

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
        output_json,
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
    if output_json {
        let payload = json!({
            "schema": "effigy.repo-pulse.v1",
            "schema_version": 1,
            "report": {
                "repo": evaluated.repo,
                "owner": evaluated.owner,
                "eta": evaluated.eta,
                "evidence": evaluated.evidence,
                "risk": evaluated.risk,
                "next_action": evaluated.next_action,
            },
            "root_resolution": {
                "resolved_root": resolved.resolved_root.display().to_string(),
                "mode": format!("{:?}", resolved.resolution_mode),
                "evidence": resolved.evidence,
                "warnings": resolved.warnings,
            }
        });
        return serde_json::to_string_pretty(&payload)
            .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")));
    }
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
    let precedence = vec![
        "explicit catalog alias prefix".to_owned(),
        "relative/absolute catalog path prefix".to_owned(),
        "unprefixed nearest in-scope catalog by cwd".to_owned(),
        "unprefixed shallowest catalog from workspace root".to_owned(),
    ];

    let resolve_probe = if let Some(raw_selector) = args.resolve_selector.clone() {
        let (selector, selector_args) = parse_task_reference_invocation(&raw_selector)?;
        let selector_task_name = selector.task_name.clone();
        let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
        match select_catalog_and_task(&selector, &catalogs, &cwd) {
            Ok(selection) => {
                if selection.task.mode.as_deref() == Some("tui") {
                    let profile_name = selector_args
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "default".to_owned());
                    if !concurrent_entries_for_profile(selection.task, &profile_name) {
                        let available = available_concurrent_profiles(selection.task);
                        let available_display = if available.is_empty() {
                            "<none>".to_owned()
                        } else {
                            available.join(", ")
                        };
                        Some(json!({
                            "selector": raw_selector,
                            "status": "error",
                            "catalog": serde_json::Value::Null,
                            "catalog_root": serde_json::Value::Null,
                            "task": selector_task_name,
                            "evidence": Vec::<String>::new(),
                            "error": format!(
                                "managed profile `{profile_name}` not found for task `{}`; available: {}",
                                selector_task_name,
                                available_display
                            ),
                        }))
                    } else {
                        let mut evidence = selection.evidence.clone();
                        evidence.push(format!(
                            "managed profile `{profile_name}` resolved via invocation `{raw_selector}`"
                        ));
                        Some(json!({
                            "selector": raw_selector,
                            "status": "ok",
                            "catalog": selection.catalog.alias,
                            "catalog_root": selection.catalog.catalog_root.display().to_string(),
                            "task": selector_task_name,
                            "evidence": evidence,
                            "error": serde_json::Value::Null,
                        }))
                    }
                } else {
                    Some(json!({
                        "selector": raw_selector,
                        "status": "ok",
                        "catalog": selection.catalog.alias,
                        "catalog_root": selection.catalog.catalog_root.display().to_string(),
                        "task": selector_task_name,
                        "evidence": selection.evidence,
                        "error": serde_json::Value::Null,
                    }))
                }
            }
            Err(error) => {
                if BUILTIN_TASKS
                    .iter()
                    .any(|(name, _)| *name == selector_task_name.as_str())
                    || selector_task_name == "catalogs"
                {
                    Some(json!({
                        "selector": raw_selector,
                        "status": "ok",
                        "catalog": serde_json::Value::Null,
                        "catalog_root": serde_json::Value::Null,
                        "task": selector_task_name.clone(),
                        "evidence": vec![format!("resolved built-in task `{}`", selector_task_name)],
                        "error": serde_json::Value::Null,
                    }))
                } else {
                    Some(json!({
                        "selector": raw_selector,
                        "status": "error",
                        "catalog": serde_json::Value::Null,
                        "catalog_root": serde_json::Value::Null,
                        "task": selector_task_name,
                        "evidence": Vec::<String>::new(),
                        "error": error.to_string(),
                    }))
                }
            }
        }
    } else {
        None
    };

    let mut ordered_catalogs = catalogs.iter().collect::<Vec<&LoadedCatalog>>();
    ordered_catalogs.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.manifest_path.cmp(&b.manifest_path))
    });
    let catalog_diagnostics = ordered_catalogs
        .iter()
        .map(|catalog| {
            json!({
                "alias": catalog.alias,
                "root": catalog.catalog_root.display().to_string(),
                "depth": catalog.depth,
                "manifest": catalog.manifest_path.display().to_string(),
                "has_defer": catalog.defer_run.is_some(),
            })
        })
        .collect::<Vec<serde_json::Value>>();

    if args.output_json {
        if let Some(filter) = args.task_name {
            let selector = parse_task_selector(&filter)?;
            let matched_tasks = catalogs
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
            let matches = matched_tasks
                .iter()
                .map(|(catalog, task)| {
                    json!({
                        "task": catalog_task_label(catalog, &selector.task_name),
                        "run": task_run_preview(task),
                        "manifest": catalog.manifest_path.display().to_string(),
                    })
                })
                .collect::<Vec<serde_json::Value>>();
            let managed_profile_matches = matched_tasks
                .iter()
                .flat_map(|(catalog, task)| {
                    managed_profile_display_rows(catalog, &selector.task_name, task)
                        .into_iter()
                        .map(|row| {
                            json!({
                                "task": row.task,
                                "run": row.run,
                                "manifest": catalog.manifest_path.display().to_string(),
                                "profile": row.profile,
                                "invocation": row.invocation,
                                "parent_task": row.parent_task,
                            })
                        })
                        .collect::<Vec<serde_json::Value>>()
                })
                .collect::<Vec<serde_json::Value>>();
            let builtin_matches = BUILTIN_TASKS
                .iter()
                .filter(|(name, _)| selector.prefix.is_none() && selector.task_name == *name)
                .map(|(name, description)| {
                    json!({
                        "task": *name,
                        "description": *description,
                    })
                })
                .collect::<Vec<serde_json::Value>>();
            let payload = json!({
                "schema": "effigy.tasks.filtered.v1",
                "schema_version": 1,
                "catalog_count": catalogs.len(),
                "filter": filter,
                "matches": matches,
                "managed_profile_matches": managed_profile_matches,
                "builtin_matches": builtin_matches,
                "catalogs": catalog_diagnostics,
                "precedence": precedence,
                "resolve": resolve_probe,
                "notes": if selector.task_name == "test" {
                    vec!["built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined".to_owned()]
                } else {
                    Vec::<String>::new()
                }
            });
            return if args.pretty_json {
                serde_json::to_string_pretty(&payload)
                    .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
            } else {
                serde_json::to_string(&payload)
                    .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
            };
        }

        let mut catalog_rows: Vec<serde_json::Value> = Vec::new();
        let mut managed_profile_rows: Vec<serde_json::Value> = Vec::new();
        for catalog in &ordered_catalogs {
            if catalog.manifest.tasks.is_empty() {
                catalog_rows.push(json!({
                    "task": null,
                    "run": null,
                    "manifest": catalog.manifest_path.display().to_string(),
                }));
                continue;
            }
            for (task_name, task_def) in &catalog.manifest.tasks {
                catalog_rows.push(json!({
                    "task": catalog_task_label(catalog, task_name),
                    "run": task_run_preview(task_def),
                    "manifest": catalog.manifest_path.display().to_string(),
                }));
                managed_profile_rows.extend(
                    managed_profile_display_rows(catalog, task_name, task_def)
                        .into_iter()
                        .map(|row| {
                            json!({
                                "task": row.task,
                                "run": row.run,
                                "manifest": catalog.manifest_path.display().to_string(),
                                "profile": row.profile,
                                "invocation": row.invocation,
                                "parent_task": row.parent_task,
                            })
                        }),
                );
            }
        }
        let builtin_rows = BUILTIN_TASKS
            .iter()
            .map(|(name, description)| {
                json!({
                    "task": *name,
                    "description": *description,
                })
            })
            .collect::<Vec<serde_json::Value>>();
        let payload = json!({
        "schema": "effigy.tasks.v1",
            "schema_version": 1,
            "catalog_count": catalogs.len(),
            "catalog_tasks": catalog_rows,
            "managed_profiles": managed_profile_rows,
            "builtin_tasks": builtin_rows,
            "catalogs": catalog_diagnostics,
            "precedence": precedence,
            "resolve": resolve_probe,
        });
        return if args.pretty_json {
            serde_json::to_string_pretty(&payload)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
        } else {
            serde_json::to_string(&payload)
                .map_err(|error| RunnerError::Ui(format!("failed to encode json: {error}")))
        };
    }

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
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

        let theme = Theme::default();
        for (catalog, task) in matches {
            let task_label = catalog_task_label(catalog, &selector.task_name);
            let manifest = relative_display_path(&resolved.resolved_root, &catalog.manifest_path);
            let signature = task_run_preview(task);
            renderer.text(&format!(
                "- {} : {}",
                style_text(color_enabled, theme.task_name, &task_label),
                style_text(color_enabled, theme.muted, &manifest),
            ))?;
            renderer.text(&format!(
                "      {}",
                style_text(color_enabled, theme.task_signature, &signature),
            ))?;
            for row in managed_profile_display_rows(catalog, &selector.task_name, task) {
                renderer.text(&format!(
                    "- {} : {}",
                    style_text(color_enabled, theme.task_name, &row.task),
                    style_text(color_enabled, theme.muted, &manifest),
                ))?;
                renderer.text(&format!(
                    "      {}",
                    style_text(color_enabled, theme.task_signature, &row.run),
                ))?;
            }
        }
        if !builtin_matches.is_empty() || resolve_probe.is_some() {
            renderer.text("")?;
        }
        if !builtin_matches.is_empty() {
            renderer.section("Built-in Task Matches")?;
            for (name, description) in builtin_matches {
                renderer.text(&format!(
                    "- {} : {}",
                    style_text(color_enabled, theme.task_name, name),
                    style_text(color_enabled, theme.muted, description),
                ))?;
            }
            if selector.task_name == "test" {
                renderer.notice(
                    NoticeLevel::Info,
                    "built-in fallback supports `<catalog>/test` when explicit `tasks.test` is not defined",
                )?;
            }
            if resolve_probe.is_some() {
                renderer.text("")?;
            }
        }
        if let Some(probe) = resolve_probe {
            renderer.section(&format!(
                "Resolution: {}",
                probe["selector"].as_str().unwrap_or("<selector>")
            ))?;
            renderer.key_values(&[
                KeyValue::new("status", probe["status"].as_str().unwrap_or("<unknown>")),
                KeyValue::new("catalog", probe["catalog"].as_str().unwrap_or("<none>")),
                KeyValue::new("task", probe["task"].as_str().unwrap_or("<none>")),
            ])?;
            if let Some(error) = probe["error"].as_str() {
                renderer.notice(NoticeLevel::Warning, error)?;
            }
        }
        let out = renderer.into_inner();
        return String::from_utf8(out).map_err(|error| {
            RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}"))
        });
    }

    if let Some(probe) = resolve_probe.as_ref() {
        let theme = Theme::default();
        renderer.section(&format!(
            "Resolution: {}",
            probe["selector"].as_str().unwrap_or("<selector>")
        ))?;
        renderer.key_values(&[
            KeyValue::new("status", probe["status"].as_str().unwrap_or("<unknown>")),
            KeyValue::new("catalog", probe["catalog"].as_str().unwrap_or("<none>")),
            KeyValue::new("task", probe["task"].as_str().unwrap_or("<none>")),
        ])?;
        if let Some(error) = probe["error"].as_str() {
            renderer.notice(NoticeLevel::Warning, error)?;
        } else if let Some(evidence) = probe["evidence"].as_array() {
            let lines = evidence
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<String>>();
            if !lines.is_empty() {
                renderer.text(&format!(
                    "{}:",
                    style_text(color_enabled, theme.label, "evidence")
                ))?;
                for line in lines {
                    renderer.text(&format!("- {line}"))?;
                }
            }
        }
        let out = renderer.into_inner();
        return String::from_utf8(out)
            .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")));
    }

    renderer.section("Catalogs")?;
    renderer.key_values(&[KeyValue::new("count", catalogs.len().to_string())])?;
    let theme = Theme::default();
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        for catalog in &ordered_catalogs {
            let manifest = relative_display_path(&resolved.resolved_root, &catalog.manifest_path);
            renderer.text(&format!(
                "- {} : {}",
                style_text(color_enabled, theme.task_name, &catalog.alias),
                style_text(color_enabled, theme.muted, &manifest),
            ))?;
        }
    }
    renderer.text("")?;

    renderer.section("Tasks")?;
    let mut has_tasks = false;
    if ordered_catalogs.is_empty() {
        renderer.notice(NoticeLevel::Info, "none")?;
    } else {
        for catalog in &ordered_catalogs {
            if catalog.manifest.tasks.is_empty() {
                continue;
            }
            let manifest = relative_display_path(&resolved.resolved_root, &catalog.manifest_path);
            for (task_name, task_def) in &catalog.manifest.tasks {
                let task_label = catalog_task_label(catalog, task_name);
                let signature = task_run_preview(task_def);
                renderer.text(&format!(
                    "- {} : {}",
                    style_text(color_enabled, theme.task_name, &task_label),
                    style_text(color_enabled, theme.muted, &manifest),
                ))?;
                renderer.text(&format!(
                    "      {}",
                    style_text(color_enabled, theme.task_signature, &signature),
                ))?;
                has_tasks = true;
                for row in managed_profile_display_rows(catalog, task_name, task_def) {
                    renderer.text(&format!(
                        "- {} : {}",
                        style_text(color_enabled, theme.task_name, &row.task),
                        style_text(color_enabled, theme.muted, &manifest),
                    ))?;
                    renderer.text(&format!(
                        "      {}",
                        style_text(color_enabled, theme.task_signature, &row.run),
                    ))?;
                }
            }
        }
    }
    if !has_tasks {
        renderer.notice(NoticeLevel::Info, "none")?;
    }
    renderer.text("")?;

    renderer.section("Built-in Tasks")?;
    for (name, description) in BUILTIN_TASKS {
        renderer.text(&format!(
            "- {} : {}",
            style_text(color_enabled, theme.task_name, name),
            style_text(color_enabled, theme.muted, description),
        ))?;
    }
    if resolve_probe.is_some() {
        renderer.text("")?;
    }

    if let Some(probe) = resolve_probe {
        let theme = Theme::default();
        renderer.section(&format!(
            "Resolution: {}",
            probe["selector"].as_str().unwrap_or("<selector>")
        ))?;
        renderer.key_values(&[
            KeyValue::new("status", probe["status"].as_str().unwrap_or("<unknown>")),
            KeyValue::new("catalog", probe["catalog"].as_str().unwrap_or("<none>")),
            KeyValue::new("task", probe["task"].as_str().unwrap_or("<none>")),
        ])?;
        if let Some(error) = probe["error"].as_str() {
            renderer.notice(NoticeLevel::Warning, error)?;
        } else if let Some(evidence) = probe["evidence"].as_array() {
            let lines = evidence
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect::<Vec<String>>();
            if !lines.is_empty() {
                renderer.text(&format!(
                    "{}:",
                    style_text(color_enabled, theme.label, "evidence")
                ))?;
                for line in lines {
                    renderer.text(&format!("- {line}"))?;
                }
            }
        }
    }
    let out = renderer.into_inner();
    return String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")));
}

fn concurrent_entries_for_profile(task: &ManifestTask, profile_name: &str) -> bool {
    if task
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.concurrent_entries())
        .is_some()
    {
        return true;
    }
    profile_name == "default" && !task.concurrent.is_empty()
}

fn available_concurrent_profiles(task: &ManifestTask) -> Vec<String> {
    let mut available = task
        .profiles
        .iter()
        .filter_map(|(name, profile)| {
            profile
                .concurrent_entries()
                .is_some()
                .then_some(name.clone())
        })
        .collect::<Vec<String>>();
    if !task.concurrent.is_empty() && !available.iter().any(|name| name == "default") {
        available.push("default".to_owned());
    }
    available.sort();
    available
}

fn relative_display_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|relative| relative.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

fn managed_profile_display_rows(
    catalog: &LoadedCatalog,
    task_name: &str,
    task: &ManifestTask,
) -> Vec<ManagedProfileDisplayRow> {
    let Some(mode) = task.mode.as_deref() else {
        return Vec::new();
    };
    if task.profiles.is_empty() {
        return Vec::new();
    }
    let parent_task = catalog_task_label(catalog, task_name);
    task.profiles
        .keys()
        .filter(|profile| profile.as_str() != "default")
        .map(|profile| ManagedProfileDisplayRow {
            task: format!("{parent_task} {profile}"),
            run: format!("<managed:{mode} profile:{profile}>"),
            profile: profile.clone(),
            invocation: format!("{parent_task} {profile}"),
            parent_task: parent_task.clone(),
        })
        .collect()
}

fn style_text(enabled: bool, style: anstyle::Style, text: &str) -> String {
    if !enabled {
        return text.to_owned();
    }
    format!("{}{}{}", style.render(), text, style.render_reset())
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

#[cfg(test)]
#[path = "../tests/catalogs_contract_tests.rs"]
mod catalogs_contract_tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;
