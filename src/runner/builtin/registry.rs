use std::path::Path;

use crate::TaskInvocation;

use super::super::model::catalog::{LoadedCatalog, TaskRuntimeArgs, TaskSelector};
use super::{
    cache, completion, config, doctor, help, init, migrate, scan, tasks, test, unlock, watch,
};
use crate::runner::error::RunnerError;

pub(super) struct BuiltinRegistryEntry {
    pub(super) name: &'static str,
    dispatch: BuiltinDispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinDispatch {
    Doctor,
    Catalogs,
    Tasks,
    Config,
    Help,
    Watch,
    Init,
    Migrate,
    Unlock,
    Cache,
    Completion,
    Scan,
    Test,
}

const BUILTIN_REGISTRY: [BuiltinRegistryEntry; 13] = [
    BuiltinRegistryEntry {
        name: "doctor",
        dispatch: BuiltinDispatch::Doctor,
    },
    BuiltinRegistryEntry {
        name: "catalogs",
        dispatch: BuiltinDispatch::Catalogs,
    },
    BuiltinRegistryEntry {
        name: "tasks",
        dispatch: BuiltinDispatch::Tasks,
    },
    BuiltinRegistryEntry {
        name: "config",
        dispatch: BuiltinDispatch::Config,
    },
    BuiltinRegistryEntry {
        name: "help",
        dispatch: BuiltinDispatch::Help,
    },
    BuiltinRegistryEntry {
        name: "watch",
        dispatch: BuiltinDispatch::Watch,
    },
    BuiltinRegistryEntry {
        name: "init",
        dispatch: BuiltinDispatch::Init,
    },
    BuiltinRegistryEntry {
        name: "migrate",
        dispatch: BuiltinDispatch::Migrate,
    },
    BuiltinRegistryEntry {
        name: "unlock",
        dispatch: BuiltinDispatch::Unlock,
    },
    BuiltinRegistryEntry {
        name: "cache",
        dispatch: BuiltinDispatch::Cache,
    },
    BuiltinRegistryEntry {
        name: "completion",
        dispatch: BuiltinDispatch::Completion,
    },
    BuiltinRegistryEntry {
        name: "scan",
        dispatch: BuiltinDispatch::Scan,
    },
    BuiltinRegistryEntry {
        name: "test",
        dispatch: BuiltinDispatch::Test,
    },
];

pub(super) fn builtin_registry_entry(task_name: &str) -> Option<&'static BuiltinRegistryEntry> {
    BUILTIN_REGISTRY
        .iter()
        .find(|entry| entry.name == task_name)
}

impl BuiltinRegistryEntry {
    pub(super) fn run(
        &self,
        selector: &TaskSelector,
        task: &TaskInvocation,
        runtime_args: &TaskRuntimeArgs,
        target_root: &Path,
        catalogs: &[LoadedCatalog],
        invocation_cwd: &Path,
    ) -> Result<Option<String>, RunnerError> {
        self.dispatch.run(
            selector,
            task,
            runtime_args,
            target_root,
            catalogs,
            invocation_cwd,
        )
    }
}

impl BuiltinDispatch {
    fn run(
        self,
        selector: &TaskSelector,
        task: &TaskInvocation,
        runtime_args: &TaskRuntimeArgs,
        target_root: &Path,
        catalogs: &[LoadedCatalog],
        invocation_cwd: &Path,
    ) -> Result<Option<String>, RunnerError> {
        match self {
            Self::Doctor => doctor::run_builtin_doctor(task, runtime_args, target_root),
            Self::Catalogs => tasks::run_builtin_tasks(task, runtime_args, target_root, true),
            Self::Tasks => tasks::run_builtin_tasks(task, runtime_args, target_root, false),
            Self::Config => config::run_builtin_config(task, &runtime_args.passthrough),
            Self::Help => help::run_builtin_help(task, &runtime_args.passthrough, target_root),
            Self::Watch => watch::run_builtin_watch(task, runtime_args, target_root),
            Self::Init => init::run_builtin_init(task, &runtime_args.passthrough, target_root),
            Self::Migrate => {
                migrate::run_builtin_migrate(task, &runtime_args.passthrough, target_root)
            }
            Self::Unlock => {
                unlock::run_builtin_unlock(task, &runtime_args.passthrough, target_root)
            }
            Self::Cache => {
                cache::run_builtin_cache(task, runtime_args, target_root, catalogs, invocation_cwd)
            }
            Self::Completion => completion::run_builtin_completion(task, runtime_args, target_root),
            Self::Scan => scan::run_builtin_scan(task, runtime_args, target_root, catalogs),
            Self::Test => {
                test::try_run_builtin_test(selector, task, runtime_args, target_root, catalogs)
            }
        }
    }
}

#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;
