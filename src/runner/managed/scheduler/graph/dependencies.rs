use std::collections::BTreeMap;

use super::super::super::super::manifest::task_runtime::ManifestManagedRunStep;
use crate::runner::error::RunnerError;

pub(super) fn build_step_dependencies(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    id_to_index: &BTreeMap<String, usize>,
) -> Result<Vec<Vec<usize>>, RunnerError> {
    steps
        .iter()
        .enumerate()
        .map(|(index, step)| step_dependencies(task_name, step, index, id_to_index))
        .collect()
}

pub(super) fn build_step_dependents(dependencies: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut dependents = vec![Vec::<usize>::new(); dependencies.len()];
    for (index, deps) in dependencies.iter().enumerate() {
        for dep in deps {
            dependents[*dep].push(index);
        }
    }
    dependents
}

fn step_dependencies(
    task_name: &str,
    step: &ManifestManagedRunStep,
    index: usize,
    id_to_index: &BTreeMap<String, usize>,
) -> Result<Vec<usize>, RunnerError> {
    let mut dependencies = match step {
        ManifestManagedRunStep::Command(_) => implicit_sequential_dependencies(index),
        ManifestManagedRunStep::Step(table) if table.depends_on.is_empty() => {
            implicit_sequential_dependencies(index)
        }
        ManifestManagedRunStep::Step(table) => resolve_explicit_dependencies(
            task_name,
            table.id.as_deref(),
            table.depends_on.as_slice(),
            index,
            id_to_index,
        )?,
    };
    dependencies.sort_unstable();
    dependencies.dedup();
    Ok(dependencies)
}

fn implicit_sequential_dependencies(index: usize) -> Vec<usize> {
    if index == 0 {
        return Vec::new();
    }
    vec![index - 1]
}

fn resolve_explicit_dependencies(
    task_name: &str,
    raw_step_id: Option<&str>,
    raw_dependencies: &[String],
    step_index: usize,
    id_to_index: &BTreeMap<String, usize>,
) -> Result<Vec<usize>, RunnerError> {
    let step_id = require_step_id_for_dependencies(task_name, raw_step_id, step_index)?;
    let mut dependencies = Vec::<usize>::with_capacity(raw_dependencies.len());
    for raw_dep in raw_dependencies {
        let dep = raw_dep.trim();
        if dep.is_empty() {
            return Err(RunnerError::task_invocation(format!(
                "task `{task_name}` run step `{step_id}` has an empty dependency in `depends_on`"
            )));
        }
        let Some(dep_index) = id_to_index.get(dep).copied() else {
            return Err(RunnerError::task_invocation(format!(
                "task `{task_name}` run step `{step_id}` depends on missing step `{dep}`"
            )));
        };
        if dep_index == step_index {
            return Err(RunnerError::task_invocation(format!(
                "task `{task_name}` run step `{step_id}` cannot depend on itself"
            )));
        }
        dependencies.push(dep_index);
    }
    Ok(dependencies)
}

fn require_step_id_for_dependencies<'a>(
    task_name: &str,
    raw_step_id: Option<&'a str>,
    step_index: usize,
) -> Result<&'a str, RunnerError> {
    let step_id = raw_step_id.map(str::trim).unwrap_or_default();
    if step_id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` run step {} defines `depends_on` but is missing a non-empty `id`",
            step_index + 1
        )));
    }
    Ok(step_id)
}
