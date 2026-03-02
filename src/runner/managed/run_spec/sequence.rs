use std::path::Path;

use super::super::super::{LoadedCatalog, ManifestManagedRunStep, RunnerError};
use super::super::scheduler;
use super::run_step::resolve_task_run_step;

pub(super) fn render_run_sequence(
    task_name: &str,
    steps: &[ManifestManagedRunStep],
    args_rendered: &str,
    repo_root: &Path,
    catalogs: &[LoadedCatalog],
    task_scope_cwd: &Path,
    depth: usize,
) -> Result<String, RunnerError> {
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
            depth,
        )?);
        policies.push(scheduler::step_policy_for(step));
    }
    let has_non_default_policy = policies.iter().copied().any(|policy| !policy.is_default());
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
