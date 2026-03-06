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
mod scan;
mod tasks_command;
mod tasks_diagnostics;
mod tasks_listing;
mod tasks_probe;
mod tasks_view;
#[cfg(test)]
mod test_support;
mod tooling;
mod util;

use builtin::try_run_builtin_task;
use manifest::{
    ManifestGodFilesConfig, ManifestJsPackageManager, ManifestManagedConcurrentEntry,
    ManifestManagedRun, ManifestManagedRunStep, ManifestScanOutputFormat, ManifestTask,
    ManifestTaskCache, TaskManifest,
};
use model::{
    CatalogSelectionMode, DeferredCommand, LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan,
    TaskRuntimeArgs, TaskSelection, TaskSelector, BUILTIN_TASKS, DEFAULT_MANAGED_SHELL_RUN,
    DEFER_DEPTH_ENV, IMPLICIT_ROOT_DEFER_TEMPLATE, TASK_MANIFEST_FILE,
};

pub(super) const DEFAULT_BUILTIN_TEST_MAX_PARALLEL: usize =
    model::DEFAULT_BUILTIN_TEST_MAX_PARALLEL;

pub use entrypoints::{resolve_command_root, run_command, run_doctor, run_tasks};
pub use error::RunnerError;

#[cfg(test)]
#[path = "../tests/runner_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/catalogs_contract_tests.rs"]
mod catalogs_contract_tests;

#[cfg(test)]
#[path = "../tests/json_contract_tests/mod.rs"]
mod json_contract_tests;

#[cfg(test)]
#[path = "../tests/task_ref_parser_tests.rs"]
mod task_ref_parser_tests;

#[cfg(test)]
#[path = "../tests/cache_tests.rs"]
mod cache_tests;
