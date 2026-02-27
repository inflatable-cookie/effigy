use std::path::{Path, PathBuf};

use crate::{TaskInvocation, BUILTIN_TASKS};

use super::{LoadedCatalog, RunnerError, TaskRuntimeArgs, TaskSelector};

mod help;
mod pulse;
mod tasks;
mod test;

fn is_builtin_task(task_name: &str) -> bool {
    BUILTIN_TASKS.iter().any(|(name, _)| *name == task_name)
}

fn resolve_builtin_task_target_root(
    selector: &TaskSelector,
    resolved_root: &Path,
    catalogs: &[LoadedCatalog],
) -> Option<PathBuf> {
    if let Some(prefix) = selector.prefix.as_ref() {
        return catalogs
            .iter()
            .find(|catalog| &catalog.alias == prefix)
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
) -> Result<Option<String>, RunnerError> {
    if !is_builtin_task(&selector.task_name) {
        return Ok(None);
    }

    let Some(target_root) = resolve_builtin_task_target_root(selector, resolved_root, catalogs)
    else {
        return Ok(None);
    };

    match selector.task_name.as_str() {
        "health" | "repo-pulse" => {
            pulse::run_builtin_repo_pulse(task, runtime_args, &target_root).map(Some)
        }
        "tasks" => tasks::run_builtin_tasks(task, runtime_args, &target_root).map(Some),
        "help" => help::run_builtin_help(),
        "test" => test::try_run_builtin_test(selector, task, runtime_args, &target_root, catalogs),
        _ => Ok(None),
    }
}

pub(super) fn builtin_test_max_parallel(catalogs: &[LoadedCatalog], resolved_root: &Path) -> usize {
    test::builtin_test_max_parallel(catalogs, resolved_root)
}
