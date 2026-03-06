use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::super::manifest::task_runtime::ManifestManagedConcurrentEntry;
use super::super::super::{
    LoadedCatalog, ManagedProcessSpec, TaskSelector, DEFAULT_MANAGED_SHELL_RUN,
};
use super::super::references;
use super::{invalid_managed_process_definition, select_run_or_task};

pub(super) fn resolve_concurrent_process_entries(
    selector: &TaskSelector,
    entries: &[ManifestManagedConcurrentEntry],
    catalog: &LoadedCatalog,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Vec<super::ConcurrentResolvedProcess>, crate::runner::error::RunnerError> {
    let mut used_names = HashSet::<String>::new();
    let mut resolved = Vec::<super::ConcurrentResolvedProcess>::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let normalized = normalize_entry(entry, index);
        let process_name = normalized.process_name;
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
        resolved.push(super::ConcurrentResolvedProcess {
            spec: ManagedProcessSpec {
                name: process_name,
                run,
                cwd,
                start_after_ms: normalized.start_after_ms,
            },
            start_rank: normalized.start_rank,
            tab_rank: normalized.tab_rank,
            index,
        });
    }
    Ok(resolved)
}

struct NormalizedConcurrentEntry {
    process_name: String,
    start_rank: usize,
    tab_rank: usize,
    start_after_ms: u64,
}

fn normalize_entry(
    entry: &ManifestManagedConcurrentEntry,
    index: usize,
) -> NormalizedConcurrentEntry {
    let ordinal = index + 1;
    let process_name = entry
        .name
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| entry.task.clone())
        .unwrap_or_else(|| format!("process-{ordinal}"));
    let start_rank = entry.start.unwrap_or(ordinal);
    let tab_rank = entry.tab.unwrap_or(start_rank);
    NormalizedConcurrentEntry {
        process_name,
        start_rank,
        tab_rank,
        start_after_ms: entry.start_after_ms.unwrap_or(0),
    }
}

fn resolve_process_run_and_cwd(
    selector: &TaskSelector,
    process_name: &str,
    entry: &ManifestManagedConcurrentEntry,
    catalog: &LoadedCatalog,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), crate::runner::error::RunnerError> {
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
        super::RunOrTaskRef::Task(task_ref) => resolve_task_process_run_and_cwd(
            selector,
            process_name,
            task_ref,
            catalog,
            catalogs,
            task_scope_cwd,
        ),
        super::RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}

fn resolve_task_process_run_and_cwd(
    selector: &TaskSelector,
    process_name: &str,
    task_ref: &str,
    catalog: &LoadedCatalog,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), crate::runner::error::RunnerError> {
    if task_ref.trim() == "shell" {
        return resolve_shell_process_run(selector, process_name, catalog, task_scope_cwd);
    }
    references::resolve_task_reference_run(
        &selector.task_name,
        process_name,
        task_ref,
        catalogs,
        task_scope_cwd,
    )
}

fn resolve_shell_process_run(
    selector: &TaskSelector,
    process_name: &str,
    catalog: &LoadedCatalog,
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), crate::runner::error::RunnerError> {
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
    Ok((shell_run, task_scope_cwd.to_path_buf()))
}
