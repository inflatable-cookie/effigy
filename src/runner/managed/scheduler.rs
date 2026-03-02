use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::super::util::shell_quote;
use super::super::{ManifestManagedRunStep, RunnerError};

const DEFAULT_DAG_MAX_PARALLEL: usize = 4;

#[derive(Clone, Copy)]
pub(super) struct RunStepPolicy {
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
    pub(super) fn is_default(self) -> bool {
        self.timeout_ms.is_none() && self.retry == 0 && self.retry_delay_ms == 0 && self.fail_fast
    }
}

pub(super) fn build_run_sequence_schedule(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<Option<Vec<Vec<usize>>>, RunnerError> {
    let step_index = build_step_index(task_name, steps)?;
    if !step_index.has_explicit_dependencies {
        return Ok(None);
    }

    let dependencies = build_step_dependencies(task_name, steps, &step_index.id_to_index)?;
    let dependents = build_step_dependents(&dependencies);

    if let Some(cycle) = detect_dependency_cycle(&dependencies, &step_index.display_names) {
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

struct StepIndex {
    has_explicit_dependencies: bool,
    id_to_index: BTreeMap<String, usize>,
    display_names: Vec<String>,
}

fn build_step_index(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<StepIndex, RunnerError> {
    let mut has_explicit_dependencies = false;
    let mut declared_ids = HashSet::<String>::new();
    let mut id_to_index = BTreeMap::<String, usize>::new();
    let mut display_names = Vec::<String>::with_capacity(steps.len());

    for (index, step) in steps.iter().enumerate() {
        match step {
            ManifestManagedRunStep::Command(_) => {
                display_names.push(default_step_name(index));
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
                    display_names.push(default_step_name(index));
                }
                if !table.depends_on.is_empty() {
                    has_explicit_dependencies = true;
                }
            }
        }
    }

    Ok(StepIndex {
        has_explicit_dependencies,
        id_to_index,
        display_names,
    })
}

fn build_step_dependencies(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    id_to_index: &BTreeMap<String, usize>,
) -> Result<Vec<Vec<usize>>, RunnerError> {
    let mut dependencies = vec![Vec::<usize>::new(); steps.len()];
    for (index, step) in steps.iter().enumerate() {
        dependencies[index] = step_dependencies(task_name, step, index, id_to_index)?;
    }
    Ok(dependencies)
}

fn step_dependencies(
    task_name: &str,
    step: &ManifestManagedRunStep,
    index: usize,
    id_to_index: &BTreeMap<String, usize>,
) -> Result<Vec<usize>, RunnerError> {
    let mut dependencies = Vec::<usize>::new();
    match step {
        ManifestManagedRunStep::Command(_) => {
            if index > 0 {
                dependencies.push(index - 1);
            }
        }
        ManifestManagedRunStep::Step(table) => {
            if table.depends_on.is_empty() {
                if index > 0 {
                    dependencies.push(index - 1);
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
                    dependencies.push(dep_index);
                }
            }
        }
    }
    dependencies.sort_unstable();
    dependencies.dedup();
    Ok(dependencies)
}

fn build_step_dependents(dependencies: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut dependents = vec![Vec::<usize>::new(); dependencies.len()];
    for (index, deps) in dependencies.iter().enumerate() {
        for dep in deps {
            dependents[*dep].push(index);
        }
    }
    for outgoing in &mut dependents {
        outgoing.sort_unstable();
    }
    dependents
}

fn default_step_name(index: usize) -> String {
    format!("step-{}", index + 1)
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

pub(super) fn render_parallel_run_levels_with_policy(
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

pub(super) fn step_policy_for(step: &ManifestManagedRunStep) -> RunStepPolicy {
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
