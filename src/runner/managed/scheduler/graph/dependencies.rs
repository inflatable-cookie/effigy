use std::collections::BTreeMap;

use super::{ManifestManagedRunStep, RunnerError};

pub(super) fn build_step_dependencies(
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

pub(super) fn build_step_dependents(dependencies: &[Vec<usize>]) -> Vec<Vec<usize>> {
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
                    return Err(RunnerError::task_invocation(format!(
                        "task `{task_name}` run step {} defines `depends_on` but is missing a non-empty `id`",
                        index + 1
                    )));
                }
                for raw_dep in &table.depends_on {
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
                    if dep_index == index {
                        return Err(RunnerError::task_invocation(format!(
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
