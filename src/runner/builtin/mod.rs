use std::path::{Path, PathBuf};

use crate::TaskInvocation;

use super::catalog::resolve_catalog_by_prefix;
use super::{LoadedCatalog, RunnerError, TaskRuntimeArgs, TaskSelector, BUILTIN_TASKS};

mod config;
mod doctor;
mod help;
mod init;
mod migrate;
mod tasks;
mod test;
mod unlock;
mod watch;

fn is_builtin_task(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name) || task_name == "catalogs"
}

fn resolve_builtin_task_target_root(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Option<PathBuf> {
    if let Some(prefix) = selector.prefix.as_ref() {
        return resolve_catalog_by_prefix(prefix, catalogs, invocation_cwd)
            .map(|catalog| catalog.catalog_root.clone());
    }
    Some(resolved_root.to_path_buf())
}

pub(super) fn try_run_builtin_task(
    selector: &TaskSelector,
    task: &TaskInvocation,
    runtime_args: &TaskRuntimeArgs,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
    invocation_cwd: &Path,
) -> Result<Option<String>, RunnerError> {
    if !is_builtin_task(&selector.task_name) {
        return Ok(None);
    }

    let Some(target_root) =
        resolve_builtin_task_target_root(selector, resolved_root, catalogs, invocation_cwd)
    else {
        return Ok(None);
    };

    match selector.task_name.as_str() {
        "doctor" => doctor::run_builtin_doctor(task, runtime_args, &target_root).map(Some),
        "catalogs" => tasks::run_builtin_tasks(task, runtime_args, &target_root, true).map(Some),
        "tasks" => tasks::run_builtin_tasks(task, runtime_args, &target_root, false).map(Some),
        "config" => config::run_builtin_config(task, &runtime_args.passthrough),
        "help" => help::run_builtin_help(task, &runtime_args.passthrough),
        "watch" => watch::run_builtin_watch(task, runtime_args, &target_root),
        "init" => init::run_builtin_init(task, &runtime_args.passthrough, &target_root),
        "migrate" => migrate::run_builtin_migrate(task, &runtime_args.passthrough, &target_root),
        "unlock" => unlock::run_builtin_unlock(task, &runtime_args.passthrough, &target_root),
        "test" => test::try_run_builtin_test(selector, task, runtime_args, &target_root, catalogs),
        _ => Ok(None),
    }
}

#[cfg(test)]
pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    test::builtin_test_max_parallel(catalogs, resolved_root)
}
