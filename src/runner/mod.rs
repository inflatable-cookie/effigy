use std::path::PathBuf;

use crate::process_manager::ProcessManagerError;
use crate::resolver::ResolveError;
use crate::tasks::TaskError;
#[cfg(test)]
pub(crate) use crate::{DoctorArgs, TaskInvocation, TasksArgs};

mod bridges;
mod builtin;
mod cache;
mod catalog;
mod command_context;
mod deferral;
mod doctor;
mod entrypoints;
mod error;
mod execute;
mod locking;
mod managed;
mod manifest;
mod model;
mod render;
mod tasks_command;
mod tasks_diagnostics;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
mod util;

use builtin::try_run_builtin_task;
#[cfg(test)]
use catalog::discover_catalogs;
use manifest::{
    ManifestJsPackageManager, ManifestManagedConcurrentEntry, ManifestManagedRun,
    ManifestManagedRunStep, ManifestTask, ManifestTaskCache, TaskManifest,
};
use model::{
    CatalogSelectionMode, DeferredCommand, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    TaskRuntimeArgs, TaskSelection, TaskSelector, BUILTIN_TASKS, DEFAULT_MANAGED_SHELL_RUN,
    DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE, TASK_MANIFEST_FILE,
};
#[cfg(test)]
use util::parse_task_reference_invocation;
#[cfg(test)]
use util::parse_task_runtime_args;
#[cfg(test)]
use util::parse_task_selector;

pub(super) const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize =
    model::DEFAULT_BUILTIN_TEST_MAX_PARALLEL;

#[cfg(test)]
use bridges::builtin_test_max_parallel;
use bridges::run_manifest_task_with_cwd;
pub use entrypoints::{resolve_command_root, run_command, run_doctor, run_tasks};

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
