use std::path::PathBuf;

use crate::process_manager::ProcessManagerError;
use crate::resolver::{resolve_target_root, ResolveError};
use crate::tasks::TaskError;
use crate::TaskInvocation;
use crate::{Command, DoctorArgs, TasksArgs};

mod builtin;
mod cache;
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
mod tasks_diagnostics;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
mod util;

use builtin::try_run_builtin_task;
#[cfg(test)]
use catalog::discover_catalogs;
use catalog::discover_catalogs_allow_missing;
use execute::run_manifest_task;
use manifest::{
    ManifestJsPackageManager, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestManagedRunStep, ManifestTask, ManifestTaskCache, TaskManifest,
};
use model::{
    CatalogSelectionMode, DeferredCommand, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    TaskRuntimeArgs, TaskSelection, TaskSelector, BUILTIN_TASKS, DEFAULT_MANAGED_SHELL_RUN,
    DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE, TASK_MANIFEST_FILE,
};
use tasks_diagnostics::build_catalog_diagnostics;
use tasks_listing::render_tasks_listing;
use tasks_probe::build_resolve_probe;
#[cfg(test)]
use util::parse_task_reference_invocation;
use util::parse_task_runtime_args;
#[cfg(test)]
use util::parse_task_selector;

pub(super) const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize =
    model::DEFAULT_BUILTIN_TEST_MAX_PARALLEL;

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
    let repo_override = command_repo_override(cmd);

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
    let resolved = resolve_target_root(cwd, args.repo_override.clone())?;
    let catalogs = discover_catalogs_allow_missing(&resolved.resolved_root)?;
    let precedence = task_selection_precedence_notes();

    let resolve_probe = build_resolve_probe(args.resolve_selector.clone(), &catalogs)?;

    let (ordered_catalogs, catalog_diagnostics) = build_catalog_diagnostics(&catalogs);

    render_tasks_listing(
        &args,
        &catalogs,
        &ordered_catalogs,
        &catalog_diagnostics,
        &precedence,
        &resolve_probe,
        &resolved.resolved_root,
    )
}

fn command_repo_override(cmd: &Command) -> Option<PathBuf> {
    match cmd {
        Command::Doctor(args) => args.repo_override.clone(),
        Command::Tasks(args) => args.repo_override.clone(),
        Command::Task(task) => parse_task_runtime_args(&task.args)
            .ok()
            .and_then(|parsed| parsed.repo_override),
        Command::Help(_) => None,
    }
}

fn task_selection_precedence_notes() -> Vec<String> {
    [
        "explicit catalog alias prefix",
        "relative/absolute catalog path prefix",
        "unprefixed nearest in-scope catalog by cwd",
        "unprefixed shallowest catalog from workspace root",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect()
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

#[cfg(test)]
#[path = "../tests/cache_tests.rs"]
mod cache_tests;
