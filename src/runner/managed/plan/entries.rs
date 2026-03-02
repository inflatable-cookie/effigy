use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::super::{
    LoadedCatalog, ManagedProcessSpec, ManifestManagedConcurrentEntry, ManifestTask, TaskSelector,
};
use super::super::references;
use super::{invalid_managed_process_definition, select_run_or_task};

pub(super) fn resolve_concurrent_process_entries(
    selector: &TaskSelector,
    entries: &[ManifestManagedConcurrentEntry],
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Vec<super::ConcurrentResolvedProcess>, super::super::super::RunnerError> {
    let mut used_names = HashSet::<String>::new();
    let mut resolved = Vec::<super::ConcurrentResolvedProcess>::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let ordinal = index + 1;
        let process_name = process_name_for_entry(entry, ordinal);
        if !used_names.insert(process_name.clone()) {
            return Err(invalid_managed_process_definition(
                selector,
                &process_name,
                "duplicate process name; set unique `name` values in `concurrent` entries",
            ));
        }

        let (run, cwd) =
            resolve_process_run_and_cwd(selector, &process_name, entry, catalogs, task_scope_cwd)?;
        let start_rank = entry.start.unwrap_or(ordinal);
        let tab_rank = entry.tab.unwrap_or(start_rank);
        resolved.push(super::ConcurrentResolvedProcess {
            spec: ManagedProcessSpec {
                name: process_name,
                run,
                cwd,
                start_after_ms: entry.start_after_ms.unwrap_or(0),
            },
            start_rank,
            tab_rank,
            index,
        });
    }
    Ok(resolved)
}

fn process_name_for_entry(entry: &ManifestManagedConcurrentEntry, ordinal: usize) -> String {
    entry
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| entry.task.clone())
        .unwrap_or_else(|| format!("process-{ordinal}"))
}

fn resolve_process_run_and_cwd(
    selector: &TaskSelector,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), super::super::super::RunnerError> {
    match select_run_or_task(
        entry.run.as_deref(),
        entry.task.as_deref(),
        || {
            invalid_managed_process_definition(
                selector,
                process_name,
                "define either `task` or `run`, not both",
            )
        },
        || {
            invalid_managed_process_definition(
                selector,
                process_name,
                "missing both `task` and `run`",
            )
        },
    )? {
        super::RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_run(
            &selector.task_name,
            process_name,
            task_ref,
            catalogs,
            task_scope_cwd,
        ),
        super::RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}

pub(super) fn maybe_append_shell_process(
    selector: &TaskSelector,
    task: &ManifestTask,
    catalog: &LoadedCatalog,
    task_scope_cwd: &Path,
    processes: &mut Vec<ManagedProcessSpec>,
) -> Result<(), super::super::super::RunnerError> {
    if !task.shell.unwrap_or(false) {
        return Ok(());
    }
    let shell_name = "shell";
    if processes.iter().any(|process| process.name == shell_name) {
        return Err(invalid_managed_process_definition(
            selector,
            shell_name,
            "reserved process name `shell` is already defined",
        ));
    }
    let shell_run = catalog
        .manifest
        .shell
        .as_ref()
        .and_then(|shell| shell.run.clone())
        .unwrap_or_else(|| super::DEFAULT_MANAGED_SHELL_RUN.to_owned());
    processes.push(ManagedProcessSpec {
        name: shell_name.to_owned(),
        run: shell_run,
        cwd: task_scope_cwd.to_path_buf(),
        start_after_ms: 0,
    });
    Ok(())
}
