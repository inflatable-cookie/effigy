use std::collections::BTreeMap;

use super::super::super::super::manifest::task_runtime::ManifestManagedRunStep;
use crate::runner::error::RunnerError;

pub(super) struct StepIndex {
    pub(super) has_explicit_dependencies: bool,
    pub(super) id_to_index: BTreeMap<String, usize>,
    pub(super) display_names: Vec<String>,
}

pub(super) fn build_step_index(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
) -> Result<StepIndex, RunnerError> {
    let mut has_explicit_dependencies = false;
    let mut id_to_index = BTreeMap::<String, usize>::new();
    let mut display_names = Vec::<String>::with_capacity(steps.len());

    for (index, step) in steps.iter().enumerate() {
        match step {
            ManifestManagedRunStep::Command(_) => {
                display_names.push(default_step_name(index));
            }
            ManifestManagedRunStep::Step(table) => {
                let mut display_name = default_step_name(index);
                if let Some(id) = normalize_step_id(task_name, index, table.id.as_deref())? {
                    if id_to_index.insert(id.clone(), index).is_some() {
                        return Err(RunnerError::task_invocation(format!(
                            "task `{task_name}` run sequence has duplicate step id `{id}`"
                        )));
                    }
                    display_name = id;
                }
                display_names.push(display_name);
                has_explicit_dependencies |= !table.depends_on.is_empty();
            }
        }
    }

    Ok(StepIndex {
        has_explicit_dependencies,
        id_to_index,
        display_names,
    })
}

fn default_step_name(index: usize) -> String {
    format!("step-{}", index + 1)
}

fn normalize_step_id(
    task_name: &str,
    index: usize,
    raw_id: Option<&str>,
) -> Result<Option<String>, RunnerError> {
    let Some(raw_id) = raw_id else {
        return Ok(None);
    };
    let id = raw_id.trim();
    if id.is_empty() {
        return Err(RunnerError::task_invocation(format!(
            "task `{task_name}` run step {} has an empty `id`",
            index + 1
        )));
    }
    Ok(Some(id.to_owned()))
}
