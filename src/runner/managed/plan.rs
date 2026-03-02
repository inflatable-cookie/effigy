use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::super::{
    LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan, ManifestManagedConcurrentEntry,
    ManifestTask, RunnerError, TaskSelector, DEFAULT_MANAGED_SHELL_RUN,
};
use super::references;

pub(super) struct ManagedConcurrentPlanInput<'a> {
    pub(super) selector: &'a TaskSelector,
    pub(super) catalog: &'a LoadedCatalog,
    pub(super) task: &'a ManifestTask,
    pub(super) profile_name: &'a str,
    pub(super) entries: &'a [ManifestManagedConcurrentEntry],
    pub(super) passthrough: &'a [String],
    pub(super) catalogs: &'a [LoadedCatalog],
    pub(super) task_scope_cwd: &'a Path,
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

pub(super) fn resolve_managed_concurrent_task_plan(
    input: ManagedConcurrentPlanInput<'_>,
) -> Result<ManagedTaskPlan, RunnerError> {
    let ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile_name,
        entries,
        passthrough,
        catalogs,
        task_scope_cwd,
    } = input;
    if entries.is_empty() {
        return Err(RunnerError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile_name.to_owned(),
        });
    }

    let mut resolved =
        resolve_concurrent_process_entries(selector, entries, catalogs, task_scope_cwd)?;
    sort_resolved_processes(&mut resolved);
    let mut processes = resolved
        .iter()
        .map(|entry| entry.spec.clone())
        .collect::<Vec<ManagedProcessSpec>>();

    maybe_append_shell_process(selector, task, catalog, task_scope_cwd, &mut processes)?;

    let tab_order = build_tab_order(&resolved, &processes);

    Ok(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name.to_owned(),
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: passthrough.iter().skip(1).cloned().collect(),
    })
}

fn resolve_concurrent_process_entries(
    selector: &TaskSelector,
    entries: &[ManifestManagedConcurrentEntry],
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Vec<ConcurrentResolvedProcess>, RunnerError> {
    let mut used_names = HashSet::<String>::new();
    let mut resolved = Vec::<ConcurrentResolvedProcess>::with_capacity(entries.len());
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
        resolved.push(ConcurrentResolvedProcess {
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
) -> Result<(String, PathBuf), RunnerError> {
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
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_run(
            &selector.task_name,
            process_name,
            task_ref,
            catalogs,
            task_scope_cwd,
        ),
        RunOrTaskRef::Run(run) => Ok((run.to_owned(), task_scope_cwd.to_path_buf())),
    }
}

fn maybe_append_shell_process(
    selector: &TaskSelector,
    task: &ManifestTask,
    catalog: &LoadedCatalog,
    task_scope_cwd: &Path,
    processes: &mut Vec<ManagedProcessSpec>,
) -> Result<(), RunnerError> {
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
        .unwrap_or_else(|| DEFAULT_MANAGED_SHELL_RUN.to_owned());
    processes.push(ManagedProcessSpec {
        name: shell_name.to_owned(),
        run: shell_run,
        cwd: task_scope_cwd.to_path_buf(),
        start_after_ms: 0,
    });
    Ok(())
}

fn build_tab_order(
    resolved: &[ConcurrentResolvedProcess],
    processes: &[ManagedProcessSpec],
) -> Vec<String> {
    let mut tab_entries = resolved
        .iter()
        .map(|entry| (entry.spec.name.clone(), entry.tab_rank, entry.index))
        .collect::<Vec<(String, usize, usize)>>();
    tab_entries.sort_by(|a, b| {
        a.1.cmp(&b.1)
            .then_with(|| a.2.cmp(&b.2))
            .then_with(|| a.0.cmp(&b.0))
    });
    let mut tab_order = tab_entries
        .into_iter()
        .map(|(name, _, _)| name)
        .collect::<Vec<String>>();
    let mut seen = tab_order.iter().cloned().collect::<HashSet<String>>();
    for process in processes {
        if seen.insert(process.name.clone()) {
            tab_order.push(process.name.clone());
        }
    }
    tab_order
}

fn sort_resolved_processes(resolved: &mut [ConcurrentResolvedProcess]) {
    resolved.sort_by(|a, b| {
        a.start_rank
            .cmp(&b.start_rank)
            .then_with(|| a.index.cmp(&b.index))
            .then_with(|| a.spec.name.cmp(&b.spec.name))
    });
}

enum RunOrTaskRef<'a> {
    Run(&'a str),
    Task(&'a str),
}

fn select_run_or_task<'a, FBoth, FNone>(
    run: Option<&'a str>,
    task: Option<&'a str>,
    both_error: FBoth,
    none_error: FNone,
) -> Result<RunOrTaskRef<'a>, RunnerError>
where
    FBoth: FnOnce() -> RunnerError,
    FNone: FnOnce() -> RunnerError,
{
    match (run, task) {
        (Some(run), None) => Ok(RunOrTaskRef::Run(run)),
        (None, Some(task)) => Ok(RunOrTaskRef::Task(task)),
        (Some(_), Some(_)) => Err(both_error()),
        (None, None) => Err(none_error()),
    }
}

pub(super) fn invalid_managed_process_definition(
    selector: &TaskSelector,
    process: &str,
    detail: &str,
) -> RunnerError {
    RunnerError::TaskManagedProcessInvalidDefinition {
        task: selector.task_name.clone(),
        process: process.to_owned(),
        detail: detail.to_owned(),
    }
}
