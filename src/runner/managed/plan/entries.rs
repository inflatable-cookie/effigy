use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::super::{
    LoadedCatalog, ManagedProcessSpec, ManifestManagedConcurrentEntry, TaskSelector,
    DEFAULT_MANAGED_SHELL_RUN,
};
use super::super::references;
use super::{invalid_managed_process_definition, select_run_or_task};

pub(super) fn resolve_concurrent_process_entries(
    selector: &TaskSelector,
    entries: &[ManifestManagedConcurrentEntry],
    catalog: &LoadedCatalog,
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

        let (run, cwd) = resolve_process_run_and_cwd(
            selector,
            &process_name,
            entry,
            catalog,
            catalogs,
            task_scope_cwd,
        )?;
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
    catalog: &LoadedCatalog,
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
        super::RunOrTaskRef::Task(task_ref) => {
            if task_ref.trim() == "shell" {
                if process_name != "shell" {
                    return Err(invalid_managed_process_definition(
                        selector,
                        process_name,
                        "task `shell` must use process name `shell` (omit `name` or set `name = \"shell\"`)",
                    ));
                }
                let shell_run = catalog
                    .manifest
                    .shell
                    .as_ref()
                    .and_then(|shell| shell.run.clone())
                    .unwrap_or_else(|| DEFAULT_MANAGED_SHELL_RUN.to_owned());
                return Ok((shell_run, task_scope_cwd.to_path_buf()));
            }
            references::resolve_task_reference_run(
                &selector.task_name,
                process_name,
                task_ref,
                catalogs,
                task_scope_cwd,
            )
        }
        super::RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}
