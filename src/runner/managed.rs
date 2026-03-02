use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::runner::manifest::ManifestManagedRunStepTable;

use super::util::shell_quote;
use super::{
    LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan, ManifestManagedConcurrentEntry,
    ManifestManagedRun, ManifestManagedRunStep, ManifestTask, RunnerError, TaskRuntimeArgs,
    TaskSelector, DEFAULT_MANAGED_SHELL_RUN,
};

mod presentation;
mod references;
mod runtime;
mod scheduler;

const DEFAULT_MANAGED_PROFILE: &str = "default";

pub(super) fn resolve_managed_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    runtime_args: &TaskRuntimeArgs,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<Option<ManagedTaskPlan>, RunnerError> {
    let Some(mode) = task.mode.as_deref() else {
        return Ok(None);
    };
    if mode != "tui" {
        return Err(RunnerError::TaskManagedUnsupportedMode {
            task: selector.task_name.clone(),
            mode: mode.to_owned(),
        });
    }

    let profile_name = requested_profile_name(runtime_args);

    let entries = select_concurrent_entries(selector, task, &profile_name)?;
    resolve_managed_concurrent_task_plan(ManagedConcurrentPlanInput {
        selector,
        catalog,
        task,
        profile_name: &profile_name,
        entries,
        passthrough: &runtime_args.passthrough,
        catalogs,
        task_scope_cwd,
    })
    .map(Some)
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

struct ManagedConcurrentPlanInput<'a> {
    selector: &'a TaskSelector,
    catalog: &'a LoadedCatalog,
    task: &'a ManifestTask,
    profile_name: &'a str,
    entries: &'a [ManifestManagedConcurrentEntry],
    passthrough: &'a [String],
    catalogs: &'a [LoadedCatalog],
    task_scope_cwd: &'a Path,
}

fn resolve_managed_concurrent_task_plan(
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

fn concurrent_entries_for_profile<'a>(
    task: &'a ManifestTask,
    profile_name: &str,
) -> Option<&'a [ManifestManagedConcurrentEntry]> {
    if let Some(entries) = task
        .profiles
        .get(profile_name)
        .and_then(|profile| profile.concurrent_entries())
    {
        return Some(entries);
    }
    if profile_name == "default" && !task.concurrent.is_empty() {
        return Some(task.concurrent.as_slice());
    }
    None
}

fn has_concurrent_schema(task: &ManifestTask) -> bool {
    !task.concurrent.is_empty()
        || task
            .profiles
            .values()
            .any(|profile| profile.concurrent_entries().is_some())
}

fn available_concurrent_profiles(task: &ManifestTask) -> Vec<String> {
    let mut available = task
        .profiles
        .iter()
        .filter_map(|(name, profile)| {
            profile
                .concurrent_entries()
                .is_some()
                .then_some(name.clone())
        })
        .collect::<Vec<String>>();
    if !task.concurrent.is_empty() && !available.iter().any(|name| name == "default") {
        available.push("default".to_owned());
    }
    available.sort();
    available
}

pub(super) fn render_task_run_spec(
    task_name: &str,
    run: &ManifestManagedRun,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    if depth > 12 {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run expansion exceeded maximum nested task references (12)"
        )));
    }
    match run {
        ManifestManagedRun::Command(command) => {
            Ok(render_command_template(command, repo_root, args_rendered))
        }
        ManifestManagedRun::Sequence(steps) => {
            if steps.is_empty() {
                return Err(RunnerError::TaskInvocation(format!(
                    "task `{task_name}` has an empty run array"
                )));
            }
            let mut commands = Vec::with_capacity(steps.len());
            let mut policies = Vec::with_capacity(steps.len());
            for step in steps {
                commands.push(resolve_task_run_step(
                    task_name,
                    step,
                    args_rendered,
                    repo_root,
                    catalogs,
                    task_scope_cwd,
                    depth + 1,
                )?);
                policies.push(scheduler::step_policy_for(step));
            }
            let has_non_default_policy =
                policies.iter().copied().any(|policy| !policy.is_default());
            let schedule = scheduler::build_run_sequence_schedule(task_name, steps)?;
            match schedule {
                Some(levels) => Ok(scheduler::render_parallel_run_levels_with_policy(
                    &commands, &levels, &policies,
                )),
                None if has_non_default_policy => {
                    let sequential_levels = (0..commands.len())
                        .map(|index| vec![index])
                        .collect::<Vec<Vec<usize>>>();
                    Ok(scheduler::render_parallel_run_levels_with_policy(
                        &commands,
                        &sequential_levels,
                        &policies,
                    ))
                }
                None => Ok(commands.join(" && ")),
            }
        }
    }
}

fn resolve_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStep,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match step {
        ManifestManagedRunStep::Command(command) => {
            if let Some(task_ref) = command
                .strip_prefix("task:")
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                references::resolve_task_reference_step(
                    task_name,
                    task_ref,
                    args_rendered,
                    catalogs,
                    task_scope_cwd,
                    depth,
                )
            } else {
                Ok(render_command_template(command, repo_root, args_rendered))
            }
        }
        ManifestManagedRunStep::Step(step) => resolve_table_task_run_step(
            task_name,
            step,
            args_rendered,
            repo_root,
            catalogs,
            task_scope_cwd,
            depth,
        ),
    }
}

fn resolve_table_task_run_step(
    task_name: &str,
    step: &ManifestManagedRunStepTable,
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    match select_run_or_task(
        step.run.as_deref(),
        step.task.as_deref(),
        || {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: define either `run` or `task`, not both"
            ))
        },
        || {
            RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: missing both `run` and `task`"
            ))
        },
    )? {
        RunOrTaskRef::Run(run) => Ok(render_command_template(run, repo_root, args_rendered)),
        RunOrTaskRef::Task(task_ref) => references::resolve_task_reference_step(
            task_name,
            task_ref,
            args_rendered,
            catalogs,
            task_scope_cwd,
            depth,
        ),
    }
}

fn render_command_template(command: &str, repo_root: &Path, args_rendered: &str) -> String {
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    command
        .replace("{repo}", &repo_rendered)
        .replace("{args}", args_rendered)
}

fn requested_profile_name(runtime_args: &TaskRuntimeArgs) -> String {
    runtime_args
        .passthrough
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MANAGED_PROFILE.to_owned())
}

fn select_concurrent_entries<'a>(
    selector: &TaskSelector,
    task: &'a ManifestTask,
    profile_name: &str,
) -> Result<&'a [ManifestManagedConcurrentEntry], RunnerError> {
    if let Some(entries) = concurrent_entries_for_profile(task, profile_name) {
        return Ok(entries);
    }
    if has_concurrent_schema(task) {
        return Err(RunnerError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name.to_owned(),
            available: available_concurrent_profiles(task),
        });
    }
    Err(invalid_managed_process_definition(
        selector,
        "concurrent",
        "managed `mode = \"tui\"` requires `concurrent = [...]` in `[tasks.<name>]` (default profile) and/or `[tasks.<name>.profiles.<profile>]`",
    ))
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

fn invalid_managed_process_definition(
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

pub(super) fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    presentation::run_or_render_managed_task(task_name, repo_root, manifest_path, plan)
}
