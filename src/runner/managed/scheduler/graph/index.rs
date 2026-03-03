use std::collections::{BTreeMap, HashSet};

use super::{ManifestManagedRunStep, RunnerError};

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

fn default_step_name(index: usize) -> String {
    format!("step-{}", index + 1)
}
