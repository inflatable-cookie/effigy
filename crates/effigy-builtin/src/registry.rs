use std::path::Path;

use effigy_cli::TaskInvocation;

use super::{config, doctor, help, init, scan, tasks, test, watch};
use crate::BuiltinError;
use crate::BuiltinRuntimePorts;
use effigy_manifest::LoadedCatalog;
use effigy_tasks::{TaskRuntimeArgs, TaskSelector};

pub(super) struct BuiltinRegistryEntry {
    pub(super) name: &'static str,
    dispatch: BuiltinDispatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuiltinDispatch {
    Doctor,
    Tasks,
    Config,
    Help,
    Watch,
    Init,
    Scan,
    Test,
}

const BUILTIN_REGISTRY: [BuiltinRegistryEntry; 8] = [
    BuiltinRegistryEntry {
        name: "doctor",
        dispatch: BuiltinDispatch::Doctor,
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
        ports: &dyn BuiltinRuntimePorts,
        selector: &TaskSelector,
        task: &TaskInvocation,
        runtime_args: &TaskRuntimeArgs,
        target_root: &Path,
        catalogs: &[LoadedCatalog],
        invocation_cwd: &Path,
    ) -> Result<Option<String>, BuiltinError> {
        self.dispatch.run(
            ports,
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
        ports: &dyn BuiltinRuntimePorts,
        selector: &TaskSelector,
        task: &TaskInvocation,
        runtime_args: &TaskRuntimeArgs,
        target_root: &Path,
        catalogs: &[LoadedCatalog],
        invocation_cwd: &Path,
    ) -> Result<Option<String>, BuiltinError> {
        match self {
            Self::Doctor => doctor::run_builtin_doctor(ports, task, runtime_args, target_root),
            Self::Tasks => tasks::run_builtin_tasks(
                ports,
                task,
                runtime_args,
                target_root,
                catalogs,
                invocation_cwd,
            ),
            Self::Config => {
                config::run_builtin_config(task, &runtime_args.passthrough, target_root)
            }
            Self::Help => {
                let deferred_builtins =
                    ports.deferred_builtins_from_catalogs(catalogs, target_root);
                help::run_builtin_help(task, &runtime_args.passthrough, &deferred_builtins)
            }
            Self::Watch => watch::run_builtin_watch(ports, task, runtime_args, target_root),
            Self::Init => init::run_builtin_init(task, &runtime_args.passthrough, target_root),
            Self::Scan => scan::run_builtin_scan(task, runtime_args, target_root, catalogs),
            Self::Test => test::try_run_builtin_test(
                ports,
                selector,
                task,
                runtime_args,
                target_root,
                catalogs,
            ),
        }
    }
}

#[cfg(test)]
#[path = "registry/tests.rs"]
mod tests;
