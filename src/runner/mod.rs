use std::io::IsTerminal;
use std::path::PathBuf;

use serde_json::json;

use crate::process_manager::ProcessManagerError;
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::TaskError;
use crate::ui::theme::{resolve_color_enabled, Theme};
use crate::ui::{KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer};
use crate::TaskInvocation;
use crate::{Command, DoctorArgs, TasksArgs};

mod builtin;
mod catalog;
mod deferral;
mod doctor;
mod error;
mod execute;
mod locking;
mod managed;
mod manifest;
mod model;
mod render;
mod tasks_probe;
mod tasks_view;
mod util;

use builtin::try_run_builtin_task;
use catalog::discover_catalogs;
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
use tasks_probe::build_resolve_probe;
use tasks_view::{
    managed_profile_display_rows, relative_display_path, render_resolution_probe_block, style_text,
};
use util::{parse_task_runtime_args, parse_task_selector};
#[cfg(test)]
use util::parse_task_reference_invocation;

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
    TaskLockConflict {
        scope: String,
        lock_path: PathBuf,
        holder_pid: Option<u32>,
        holder_started_at_epoch_ms: Option<u128>,
        remediation: String,
    },
    TaskLockIo {
        path: PathBuf,
        error: std::io::Error,
    },
    CommandJsonFailure {
        rendered: String,
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
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    DeferLoopDetected {
        depth: u8,
    },
}

pub fn run_command(cmd: Command) -> Result<String, RunnerError> {
    match cmd {
        Command::Help(_) => Ok(String::new()),
        Command::Doctor(args) => run_doctor(args),
        Command::Tasks(args) => run_tasks(args),
        Command::Task(task) => run_manifest_task(&task),
    }
}

pub fn resolve_command_root(cmd: &Command) -> PathBuf {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let repo_override = match cmd {
        Command::Doctor(args) => args.repo_override.clone(),
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

pub fn run_doctor(args: DoctorArgs) -> Result<String, RunnerError> {
    doctor::run_doctor(args)
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

    let resolve_probe = build_resolve_probe(args.resolve_selector.clone(), &catalogs)?;

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
            return render::encode_json(&payload, args.pretty_json);
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
        return render::encode_json(&payload, args.pretty_json);
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
            return render::render_utf8(renderer.into_inner());
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
            render_resolution_probe_block(&mut renderer, &probe, color_enabled, false)?;
        }
        return render::render_utf8(renderer.into_inner());
    }

    if let Some(probe) = resolve_probe.as_ref() {
        render_resolution_probe_block(&mut renderer, probe, color_enabled, true)?;
        return render::render_utf8(renderer.into_inner());
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
        render_resolution_probe_block(&mut renderer, &probe, color_enabled, true)?;
    }
    render::render_utf8(renderer.into_inner())
}

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
