use std::collections::BTreeMap;
use std::path::PathBuf;

use effigy_cli::TaskInvocation;

use super::pipeline::run_execution_pipeline;
use super::planning::build_execution_preflight;
use crate::runner::error::RunnerError;

pub(in crate::runner) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    run_execution_pipeline(task, preflight)
}

pub(in crate::runner) fn run_manifest_task_with_cwd_and_env(
    task: &TaskInvocation,
    cwd: PathBuf,
    env_overrides: &BTreeMap<String, String>,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    let selection = match super::selection::resolve_task_selection(task, &preflight)? {
        super::selection::SelectionResolution::Selected(selection) => selection,
        super::selection::SelectionResolution::Output(output) => return Ok(output),
    };

    let mut overridden_task = selection.task.clone();
    for (key, value) in env_overrides {
        overridden_task.env.insert(key.clone(), value.clone());
    }
    let overridden_selection = effigy_manifest::TaskSelection {
        catalog: selection.catalog,
        task: &overridden_task,
        mode: selection.mode,
        evidence: selection.evidence,
    };

    if let Some(output) =
        super::pipeline::managed::run_managed_task(&preflight, &overridden_selection)?
    {
        return Ok(output);
    }

    super::pipeline::standard::run_standard_task(&preflight, &overridden_selection)
}
