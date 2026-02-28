use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::process_manager::{ProcessEventKind, ProcessSpec, ProcessSupervisor};
use crate::tui::{run_multiprocess_tui, MultiProcessTuiOptions};
use crate::ui::theme::resolve_color_enabled;
use crate::ui::{
    KeyValue, NoticeLevel, OutputMode, PlainRenderer, Renderer, SummaryCounts, TableSpec,
};

use super::catalog::select_catalog_and_task;
use super::util::{parse_task_reference_invocation, render_task_selector, shell_quote};
use super::{
    LoadedCatalog, ManagedProcessSpec, ManagedTaskPlan, ManifestManagedConcurrentEntry,
    ManifestManagedRun, ManifestManagedRunStep, ManifestTask, RunnerError, TaskRuntimeArgs,
    TaskSelector, BUILTIN_TASKS, DEFAULT_MANAGED_SHELL_RUN,
};

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

    let profile_name = runtime_args
        .passthrough
        .first()
        .cloned()
        .unwrap_or_else(|| "default".to_owned());

    if let Some(entries) = concurrent_entries_for_profile(task, &profile_name) {
        return resolve_managed_concurrent_task_plan(
            selector,
            catalog,
            task,
            &profile_name,
            entries,
            &runtime_args.passthrough,
            catalogs,
            task_scope_cwd,
        )
        .map(Some);
    }

    if has_concurrent_schema(task) {
        return Err(RunnerError::TaskManagedProfileNotFound {
            task: selector.task_name.clone(),
            profile: profile_name,
            available: available_concurrent_profiles(task),
        });
    }
    Err(RunnerError::TaskManagedProcessInvalidDefinition {
        task: selector.task_name.clone(),
        process: "concurrent".to_owned(),
        detail: "managed `mode = \"tui\"` requires `concurrent = [...]` in `[tasks.<name>]` (default profile) and/or `[tasks.<name>.profiles.<profile>]`".to_owned(),
    })
}

#[derive(Debug)]
struct ConcurrentResolvedProcess {
    spec: ManagedProcessSpec,
    start_rank: usize,
    tab_rank: usize,
    index: usize,
}

fn resolve_managed_concurrent_task_plan(
    selector: &TaskSelector,
    catalog: &LoadedCatalog,
    task: &ManifestTask,
    profile_name: &str,
    entries: &[ManifestManagedConcurrentEntry],
    passthrough: &[String],
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<ManagedTaskPlan, RunnerError> {
    if entries.is_empty() {
        return Err(RunnerError::TaskManagedProfileEmpty {
            task: selector.task_name.clone(),
            profile: profile_name.to_owned(),
        });
    }

    let mut used_names = HashSet::<String>::new();
    let mut resolved = Vec::<ConcurrentResolvedProcess>::with_capacity(entries.len());
    for (index, entry) in entries.iter().enumerate() {
        let ordinal = index + 1;
        let process_name = entry
            .name
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| entry.task.clone())
            .unwrap_or_else(|| format!("process-{ordinal}"));
        if !used_names.insert(process_name.clone()) {
            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                task: selector.task_name.clone(),
                process: process_name,
                detail: "duplicate process name; set unique `name` values in `concurrent` entries"
                    .to_owned(),
            });
        }
        let (run, cwd) = match (&entry.task, &entry.run) {
            (Some(task_ref), None) => resolve_task_reference_run(
                &selector.task_name,
                &process_name,
                task_ref,
                catalogs,
                task_scope_cwd,
            )?,
            (None, Some(run)) => (run.clone(), task_scope_cwd.to_path_buf()),
            (Some(_), Some(_)) => {
                return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                    task: selector.task_name.clone(),
                    process: process_name,
                    detail: "define either `task` or `run`, not both".to_owned(),
                });
            }
            (None, None) => {
                return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                    task: selector.task_name.clone(),
                    process: process_name,
                    detail: "missing both `task` and `run`".to_owned(),
                });
            }
        };
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

    resolved.sort_by(|a, b| {
        a.start_rank
            .cmp(&b.start_rank)
            .then_with(|| a.index.cmp(&b.index))
            .then_with(|| a.spec.name.cmp(&b.spec.name))
    });
    let mut processes = resolved
        .iter()
        .map(|entry| entry.spec.clone())
        .collect::<Vec<ManagedProcessSpec>>();

    if task.shell.unwrap_or(false) {
        let shell_name = "shell".to_owned();
        if processes.iter().any(|process| process.name == shell_name) {
            return Err(RunnerError::TaskManagedProcessInvalidDefinition {
                task: selector.task_name.clone(),
                process: shell_name,
                detail: "reserved process name `shell` is already defined".to_owned(),
            });
        }
        let shell_run = catalog
            .manifest
            .shell
            .as_ref()
            .and_then(|shell| shell.run.clone())
            .unwrap_or_else(|| DEFAULT_MANAGED_SHELL_RUN.to_owned());
        processes.push(ManagedProcessSpec {
            name: "shell".to_owned(),
            run: shell_run,
            cwd: task_scope_cwd.to_path_buf(),
            start_after_ms: 0,
        });
    }

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
    for process in &processes {
        if !tab_order.iter().any(|name| name == &process.name) {
            tab_order.push(process.name.clone());
        }
    }

    Ok(ManagedTaskPlan {
        mode: "tui".to_owned(),
        profile: profile_name.to_owned(),
        processes,
        tab_order,
        fail_on_non_zero: task.fail_on_non_zero.unwrap_or(true),
        passthrough: passthrough.iter().skip(1).cloned().collect(),
    })
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

fn resolve_task_reference_run(
    managed_task_name: &str,
    process_name: &str,
    task_ref: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
) -> Result<(String, PathBuf), RunnerError> {
    let (selector, ref_args) = parse_task_reference_invocation(task_ref).map_err(|error| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: error.to_string(),
        }
    })?;
    let ref_args_rendered = ref_args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let selector_rendered = render_task_selector(&selector);
    let selection = match select_catalog_and_task(&selector, catalogs, task_scope_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if is_builtin_task_selector(&selector) {
                let command = render_builtin_task_reference_invocation(
                    &selector_rendered,
                    &ref_args_rendered,
                )?;
                return Ok((command, task_scope_cwd.to_path_buf()));
            }
            return Err(RunnerError::TaskManagedTaskReferenceInvalid {
                task: managed_task_name.to_owned(),
                process: process_name.to_owned(),
                reference: task_ref.to_owned(),
                detail: error.to_string(),
            });
        }
    };
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: managed_task_name.to_owned(),
            process: process_name.to_owned(),
            reference: task_ref.to_owned(),
            detail: format!(
                "referenced task `{}` in {} has no `run` command",
                selector.task_name,
                selection.catalog.manifest_path.display()
            ),
        }
    })?;
    let run_rendered = render_task_run_spec(
        &selector.task_name,
        run_spec,
        &ref_args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        0,
    )
    .map_err(|error| RunnerError::TaskManagedTaskReferenceInvalid {
        task: managed_task_name.to_owned(),
        process: process_name.to_owned(),
        reference: task_ref.to_owned(),
        detail: error.to_string(),
    })?;
    Ok((run_rendered, selection.catalog.catalog_root.clone()))
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
    let repo_rendered = shell_quote(&repo_root.display().to_string());
    match run {
        ManifestManagedRun::Command(command) => Ok(command
            .replace("{repo}", &repo_rendered)
            .replace("{args}", args_rendered)),
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
                policies.push(step_policy_for(step));
            }
            let has_non_default_policy =
                policies.iter().copied().any(|policy| !policy.is_default());
            let schedule = build_run_sequence_schedule(task_name, steps)?;
            match schedule {
                Some(levels) => Ok(render_parallel_run_levels_with_policy(
                    &commands, &levels, &policies,
                )),
                None if has_non_default_policy => {
                    let sequential_levels = (0..commands.len())
                        .map(|index| vec![index])
                        .collect::<Vec<Vec<usize>>>();
                    Ok(render_parallel_run_levels_with_policy(
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

const DEFAULT_DAG_MAX_PARALLEL: usize = 4;

#[derive(Clone, Copy)]
struct RunStepPolicy {
    timeout_ms: Option<u64>,
    retry: usize,
    retry_delay_ms: u64,
    fail_fast: bool,
}

impl Default for RunStepPolicy {
    fn default() -> Self {
        Self {
            timeout_ms: None,
            retry: 0,
            retry_delay_ms: 0,
            fail_fast: true,
        }
    }
}

impl RunStepPolicy {
    fn is_default(self) -> bool {
        self.timeout_ms.is_none() && self.retry == 0 && self.retry_delay_ms == 0 && self.fail_fast
    }
}

fn build_run_sequence_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<Option<Vec<Vec<usize>>>, RunnerError> {
    let mut has_explicit_dependencies = false;
    let mut declared_ids = HashSet::<String>::new();
    let mut id_to_index = BTreeMap::<String, usize>::new();
    let mut display_names = Vec::<String>::with_capacity(steps.len());

    for (index, step) in steps.iter().enumerate() {
        match step {
            ManifestManagedRunStep::Command(_) => {
                display_names.push(format!("step-{}", index + 1));
            }
            ManifestManagedRunStep::Step(table) => {
                if let Some(raw_id) = table.id.as_deref() {
                    let id = raw_id.trim();
                    if id.is_empty() {
                        return Err(RunnerError::TaskInvocation(format!(
                            "task `{task_name}` run step {} has an empty `id`",
                            index + 1
                        )));
                    }
                    if !declared_ids.insert(id.to_owned()) {
                        return Err(RunnerError::TaskInvocation(format!(
                            "task `{task_name}` run sequence has duplicate step id `{id}`"
                        )));
                    }
                    id_to_index.insert(id.to_owned(), index);
                    display_names.push(id.to_owned());
                } else {
                    display_names.push(format!("step-{}", index + 1));
                }
                if !table.depends_on.is_empty() {
                    has_explicit_dependencies = true;
                }
            }
        }
    }

    if !has_explicit_dependencies {
        return Ok(None);
    }

    let mut dependencies = vec![Vec::<usize>::new(); steps.len()];
    let mut dependents = vec![Vec::<usize>::new(); steps.len()];

    for (index, step) in steps.iter().enumerate() {
        let mut step_dependencies = Vec::<usize>::new();
        match step {
            ManifestManagedRunStep::Command(_) => {
                if index > 0 {
                    step_dependencies.push(index - 1);
                }
            }
            ManifestManagedRunStep::Step(table) => {
                if table.depends_on.is_empty() {
                    if index > 0 {
                        step_dependencies.push(index - 1);
                    }
                } else {
                    let step_id = table.id.as_deref().map(str::trim).unwrap_or_default();
                    if step_id.is_empty() {
                        return Err(RunnerError::TaskInvocation(format!(
                            "task `{task_name}` run step {} defines `depends_on` but is missing a non-empty `id`",
                            index + 1
                        )));
                    }
                    for raw_dep in &table.depends_on {
                        let dep = raw_dep.trim();
                        if dep.is_empty() {
                            return Err(RunnerError::TaskInvocation(format!(
                                "task `{task_name}` run step `{step_id}` has an empty dependency in `depends_on`"
                            )));
                        }
                        let Some(dep_index) = id_to_index.get(dep).copied() else {
                            return Err(RunnerError::TaskInvocation(format!(
                                "task `{task_name}` run step `{step_id}` depends on missing step `{dep}`"
                            )));
                        };
                        if dep_index == index {
                            return Err(RunnerError::TaskInvocation(format!(
                                "task `{task_name}` run step `{step_id}` cannot depend on itself"
                            )));
                        }
                        step_dependencies.push(dep_index);
                    }
                }
            }
        }
        step_dependencies.sort_unstable();
        step_dependencies.dedup();
        dependencies[index] = step_dependencies;
    }

    for (index, deps) in dependencies.iter().enumerate() {
        for dep in deps {
            dependents[*dep].push(index);
        }
    }
    for outgoing in &mut dependents {
        outgoing.sort_unstable();
    }

    if let Some(cycle) = detect_dependency_cycle(&dependencies, &display_names) {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run sequence contains dependency cycle: {}",
            cycle.join(" -> ")
        )));
    }

    let mut indegree = dependencies.iter().map(Vec::len).collect::<Vec<usize>>();
    let mut ready = BTreeSet::<usize>::new();
    for (index, degree) in indegree.iter().enumerate() {
        if *degree == 0 {
            ready.insert(index);
        }
    }

    let mut levels = Vec::<Vec<usize>>::new();
    let mut processed = 0usize;
    while !ready.is_empty() {
        let current = ready.iter().copied().collect::<Vec<usize>>();
        for node in &current {
            ready.remove(node);
        }
        processed += current.len();
        for node in &current {
            for dependent in &dependents[*node] {
                indegree[*dependent] = indegree[*dependent].saturating_sub(1);
                if indegree[*dependent] == 0 {
                    ready.insert(*dependent);
                }
            }
        }
        levels.push(current);
    }

    if processed != steps.len() {
        return Err(RunnerError::TaskInvocation(format!(
            "task `{task_name}` run sequence contains dependency cycle"
        )));
    }

    Ok(Some(levels))
}

fn detect_dependency_cycle(
    dependencies: &[Vec<usize>],
    display_names: &[String],
) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Visited,
    }

    fn visit(
        node: usize,
        dependencies: &[Vec<usize>],
        display_names: &[String],
        state: &mut Vec<Option<VisitState>>,
        stack: &mut Vec<usize>,
    ) -> Option<Vec<String>> {
        match state[node] {
            Some(VisitState::Visited) => return None,
            Some(VisitState::Visiting) => {
                if let Some(cycle_start) = stack.iter().position(|item| *item == node) {
                    let mut cycle = stack[cycle_start..]
                        .iter()
                        .map(|index| display_names[*index].clone())
                        .collect::<Vec<String>>();
                    cycle.push(display_names[node].clone());
                    return Some(cycle);
                }
                return Some(vec![
                    display_names[node].clone(),
                    display_names[node].clone(),
                ]);
            }
            None => {}
        }

        state[node] = Some(VisitState::Visiting);
        stack.push(node);

        for dependency in &dependencies[node] {
            if let Some(cycle) = visit(*dependency, dependencies, display_names, state, stack) {
                return Some(cycle);
            }
        }

        stack.pop();
        state[node] = Some(VisitState::Visited);
        None
    }

    let mut state = vec![None; dependencies.len()];
    let mut stack = Vec::<usize>::new();
    for node in 0..dependencies.len() {
        if let Some(cycle) = visit(node, dependencies, display_names, &mut state, &mut stack) {
            return Some(cycle);
        }
    }
    None
}

fn render_parallel_run_levels_with_policy(
    commands: &[String],
    levels: &[Vec<usize>],
    policies: &[RunStepPolicy],
) -> String {
    let max_parallel = dag_max_parallel();
    let mut lines = Vec::<String>::new();
    lines.push("__effigy_overall_status=0".to_owned());

    for level in levels {
        for batch in level.chunks(max_parallel) {
            for (offset, index) in batch.iter().enumerate() {
                lines.push(format!(
                    "({}) & __effigy_pid_{}=$!",
                    render_policy_wrapped_command(&commands[*index], policies[*index]),
                    offset + 1
                ));
            }
            for (offset, index) in batch.iter().enumerate() {
                lines.push(format!("wait \"$__effigy_pid_{}\"", offset + 1));
                lines.push("__effigy_status=$?".to_owned());
                lines.push("if [ \"$__effigy_status\" -ne 0 ]; then".to_owned());
                if policies[*index].fail_fast {
                    lines.push("  exit \"$__effigy_status\"".to_owned());
                } else {
                    lines.push("  __effigy_overall_status=1".to_owned());
                }
                lines.push("fi".to_owned());
            }
        }
    }

    lines.push("exit \"$__effigy_overall_status\"".to_owned());
    lines.join("\n")
}

fn dag_max_parallel() -> usize {
    std::env::var("EFFIGY_DAG_MAX_PARALLEL")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_DAG_MAX_PARALLEL)
}

fn step_policy_for(step: &ManifestManagedRunStep) -> RunStepPolicy {
    match step {
        ManifestManagedRunStep::Command(_) => RunStepPolicy::default(),
        ManifestManagedRunStep::Step(table) => RunStepPolicy {
            timeout_ms: table.timeout_ms,
            retry: table.retry.unwrap_or(0),
            retry_delay_ms: table.retry_delay_ms.unwrap_or(0),
            fail_fast: table.fail_fast.unwrap_or(true),
        },
    }
}

fn render_policy_wrapped_command(command: &str, policy: RunStepPolicy) -> String {
    let timeout_secs = policy
        .timeout_ms
        .map_or(0.0_f64, |value| (value as f64) / 1000.0_f64);
    let retry_delay_secs = (policy.retry_delay_ms as f64) / 1000.0_f64;
    let mut lines = Vec::<String>::new();
    lines.push("__effigy_attempt=0".to_owned());
    lines.push("while :".to_owned());
    lines.push("do".to_owned());
    if policy.timeout_ms.is_some() {
        lines.push(format!(
            "  python3 -c 'import subprocess,sys\ntry:\n r=subprocess.run([\"sh\",\"-lc\",sys.argv[2]], timeout=float(sys.argv[1]))\n sys.exit(r.returncode)\nexcept subprocess.TimeoutExpired:\n sys.exit(124)' {} {}",
            timeout_secs,
            shell_quote(command)
        ));
    } else {
        lines.push(format!("  sh -lc {}", shell_quote(command)));
    }
    lines.push("  __effigy_status=$?".to_owned());
    lines.push("  if [ \"$__effigy_status\" -eq 0 ]; then".to_owned());
    lines.push("    break".to_owned());
    lines.push("  fi".to_owned());
    lines.push(format!(
        "  if [ \"$__effigy_attempt\" -ge {} ]; then",
        policy.retry
    ));
    lines.push("    break".to_owned());
    lines.push("  fi".to_owned());
    lines.push("  __effigy_attempt=$((__effigy_attempt + 1))".to_owned());
    if policy.retry_delay_ms > 0 {
        lines.push(format!("  sleep {}", retry_delay_secs));
    }
    lines.push("done".to_owned());
    lines.push("exit \"$__effigy_status\"".to_owned());
    format!("sh -lc {}", shell_quote(&lines.join("\n")))
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
                resolve_task_reference_step(
                    task_name,
                    task_ref,
                    args_rendered,
                    catalogs,
                    task_scope_cwd,
                    depth,
                )
            } else {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(command
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
        }
        ManifestManagedRunStep::Step(step) => match (&step.run, &step.task) {
            (Some(run), None) => {
                let repo_rendered = shell_quote(&repo_root.display().to_string());
                Ok(run
                    .replace("{repo}", &repo_rendered)
                    .replace("{args}", args_rendered))
            }
            (None, Some(task_ref)) => resolve_task_reference_step(
                task_name,
                task_ref,
                args_rendered,
                catalogs,
                task_scope_cwd,
                depth,
            ),
            (Some(_), Some(_)) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: define either `run` or `task`, not both"
            ))),
            (None, None) => Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step is invalid: missing both `run` and `task`"
            ))),
        },
    }
}

fn resolve_task_reference_step(
    task_name: &str,
    task_ref: &str,
    args_rendered: &str,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
    let (selector, ref_args) = parse_task_reference_invocation(task_ref).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` is invalid: {error}"
        ))
    })?;
    let selector_rendered = render_task_selector(&selector);
    let ref_args_rendered = ref_args
        .iter()
        .map(|arg| shell_quote(arg))
        .collect::<Vec<String>>()
        .join(" ");
    let merged_args_rendered = match (ref_args_rendered.is_empty(), args_rendered.is_empty()) {
        (true, true) => String::new(),
        (false, true) => ref_args_rendered,
        (true, false) => args_rendered.to_owned(),
        (false, false) => format!("{ref_args_rendered} {args_rendered}"),
    };
    let selection = match select_catalog_and_task(&selector, catalogs, task_scope_cwd) {
        Ok(selection) => selection,
        Err(error) => {
            if is_builtin_task_selector(&selector) {
                let command = render_builtin_task_reference_invocation(
                    &selector_rendered,
                    &merged_args_rendered,
                )
                .map_err(|detail| {
                    RunnerError::TaskInvocation(format!(
                        "task `{task_name}` run step task ref `{task_ref}` failed: {detail}"
                    ))
                })?;
                return Ok(format!(
                    "(cd {} && {})",
                    shell_quote(&task_scope_cwd.display().to_string()),
                    command
                ));
            }
            return Err(RunnerError::TaskInvocation(format!(
                "task `{task_name}` run step task ref `{task_ref}` failed: {error}"
            )));
        }
    };
    let run_spec = selection.task.run.as_ref().ok_or_else(|| {
        RunnerError::TaskInvocation(format!(
            "task `{task_name}` run step task ref `{task_ref}` has no `run` command in {}",
            selection.catalog.manifest_path.display()
        ))
    })?;
    let nested = render_task_run_spec(
        &selector.task_name,
        run_spec,
        &merged_args_rendered,
        &selection.catalog.catalog_root,
        catalogs,
        &selection.catalog.catalog_root,
        depth,
    )?;
    Ok(format!(
        "(cd {} && {})",
        shell_quote(&selection.catalog.catalog_root.display().to_string()),
        nested
    ))
}

fn is_builtin_task_selector(selector: &TaskSelector) -> bool {
    BUILTIN_TASKS
        .iter()
        .any(|(name, _)| *name == selector.task_name.as_str())
}

fn render_builtin_task_reference_invocation(
    task_ref: &str,
    args_rendered: &str,
) -> Result<String, RunnerError> {
    let executable = resolve_effigy_invocation_prefix()?;
    let task = shell_quote(task_ref);
    if args_rendered.is_empty() {
        Ok(format!("{executable} {task}"))
    } else {
        Ok(format!("{executable} {task} {args_rendered}"))
    }
}

fn resolve_effigy_invocation_prefix() -> Result<String, RunnerError> {
    if let Ok(explicit) = std::env::var("EFFIGY_EXECUTABLE") {
        let trimmed = explicit.trim();
        if !trimmed.is_empty() {
            return Ok(shell_quote(trimmed));
        }
    }

    let executable = std::env::current_exe().map_err(RunnerError::Cwd)?;
    let is_test_harness = executable
        .parent()
        .and_then(|parent| parent.file_name())
        .is_some_and(|name| name == "deps");
    if is_test_harness {
        let manifest_path = shell_quote(&format!("{}/Cargo.toml", env!("CARGO_MANIFEST_DIR")));
        return Ok(format!(
            "cargo run --quiet --manifest-path {manifest_path} --bin effigy --"
        ));
    }
    Ok(shell_quote(&executable.display().to_string()))
}

fn render_managed_task_plan(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Plan")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile),
        KeyValue::new("repo-root", repo_root.display().to_string()),
        KeyValue::new("manifest", manifest_path.display().to_string()),
        KeyValue::new("processes", plan.processes.len().to_string()),
        KeyValue::new("tab-order", plan.tab_order.join(", ")),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Interactive TUI runtime is available for this task.",
    )?;
    renderer.notice(
        NoticeLevel::Info,
        "Set EFFIGY_MANAGED_STREAM=1 to run selected profile processes in stream mode.",
    )?;
    renderer.text("")?;
    let rows = plan
        .processes
        .into_iter()
        .map(|process| {
            vec![
                process.name,
                process.cwd.display().to_string(),
                process.run,
                process.start_after_ms.to_string(),
            ]
        })
        .collect::<Vec<Vec<String>>>();
    renderer.table(&TableSpec::new(
        vec![
            "process".to_owned(),
            "cwd".to_owned(),
            "run".to_owned(),
            "start-after-ms".to_owned(),
        ],
        rows,
    ))?;
    if !plan.passthrough.is_empty() {
        renderer.text("")?;
        renderer.bullet_list("profile-args", &plan.passthrough)?;
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: 1,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}

pub(super) fn run_or_render_managed_task(
    task_name: &str,
    repo_root: &Path,
    manifest_path: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let tui_override = std::env::var("EFFIGY_MANAGED_TUI").ok();
    let should_stream = std::env::var("EFFIGY_MANAGED_STREAM")
        .ok()
        .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"));
    if should_stream {
        return run_managed_task_runtime(task_name, repo_root, plan);
    }

    let should_tui = match tui_override.as_deref() {
        Some("1") => true,
        Some(value) if value.eq_ignore_ascii_case("true") => true,
        Some("0") => false,
        Some(value) if value.eq_ignore_ascii_case("false") => false,
        _ => std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
    };
    if should_tui {
        return run_managed_task_tui(task_name, repo_root, plan);
    }

    render_managed_task_plan(task_name, repo_root, manifest_path, plan)
}

fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        ..
    } = plan;
    let specs = processes
        .into_iter()
        .map(|process| ProcessSpec {
            name: process.name,
            run: process.run,
            cwd: process.cwd,
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let outcome = run_multiprocess_tui(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
    )
    .map_err(|error| {
        RunnerError::Ui(format!(
            "managed tui runtime failed for task `{task_name}`: {error}"
        ))
    })?;
    if fail_on_non_zero && !outcome.non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile,
            processes: outcome.non_zero_exits,
        });
    }
    Ok(String::new())
}

fn run_managed_task_runtime(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
) -> Result<String, RunnerError> {
    let specs = plan
        .processes
        .iter()
        .map(|process| ProcessSpec {
            name: process.name.clone(),
            run: process.run.clone(),
            cwd: process.cwd.clone(),
            start_after_ms: process.start_after_ms,
            pty: true,
        })
        .collect::<Vec<ProcessSpec>>();
    let expected = specs.len();
    let supervisor = ProcessSupervisor::spawn(repo_root.to_path_buf(), specs)?;

    let color_enabled =
        resolve_color_enabled(OutputMode::from_env(), std::io::stdout().is_terminal());
    let mut renderer = PlainRenderer::new(Vec::<u8>::new(), color_enabled);
    renderer.section("Managed Task Runtime")?;
    renderer.key_values(&[
        KeyValue::new("task", task_name.to_owned()),
        KeyValue::new("mode", plan.mode),
        KeyValue::new("profile", plan.profile.clone()),
        KeyValue::new("processes", expected.to_string()),
        KeyValue::new(
            "fail-on-non-zero",
            if plan.fail_on_non_zero {
                "enabled"
            } else {
                "disabled"
            },
        ),
    ])?;
    renderer.text("")?;
    renderer.notice(
        NoticeLevel::Info,
        "Running managed profile in temporary stream mode.",
    )?;
    renderer.text("")?;

    let mut exit_count = 0usize;
    let mut drained_after_exit = 0usize;
    let mut non_zero_exits = Vec::<(String, String)>::new();
    while exit_count < expected || drained_after_exit < 3 {
        if let Some(event) = supervisor.next_event_timeout(Duration::from_millis(100)) {
            if exit_count >= expected {
                drained_after_exit = 0;
            }
            match event.kind {
                ProcessEventKind::Stdout => {
                    renderer.text(&format!("[{}] {}", event.process, event.payload))?;
                }
                ProcessEventKind::Stderr => {
                    renderer.text(&format!("[{} stderr] {}", event.process, event.payload))?;
                }
                ProcessEventKind::StdoutChunk | ProcessEventKind::StderrChunk => {}
                ProcessEventKind::Exit => {
                    exit_count += 1;
                    if event.payload != "exit=0" {
                        non_zero_exits.push((event.process.clone(), event.payload.clone()));
                    }
                    renderer.notice(
                        NoticeLevel::Info,
                        &format!("process `{}` {}", event.process, event.payload),
                    )?;
                }
            }
        } else if exit_count >= expected {
            drained_after_exit += 1;
        }
    }

    supervisor.terminate_all();
    non_zero_exits.sort_by(|a, b| a.0.cmp(&b.0));
    non_zero_exits.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    if plan.fail_on_non_zero && !non_zero_exits.is_empty() {
        return Err(RunnerError::TaskManagedNonZeroExit {
            task: task_name.to_owned(),
            profile: plan.profile,
            processes: non_zero_exits,
        });
    }
    renderer.text("")?;
    renderer.summary(SummaryCounts {
        ok: expected,
        warn: 1,
        err: 0,
    })?;
    let out = renderer.into_inner();
    String::from_utf8(out)
        .map_err(|error| RunnerError::Ui(format!("invalid utf-8 in rendered output: {error}")))
}
