use std::path::PathBuf;

use crate::TaskInvocation;

#[path = "execute/cache_hit.rs"]
mod cache_hit;
#[path = "execute/json_payload.rs"]
mod json_payload;
#[path = "execute/pipeline.rs"]
mod pipeline;
#[path = "execute/preflight.rs"]
mod preflight;
#[path = "execute/process_run.rs"]
mod process_run;
#[path = "execute/selection.rs"]
mod selection;

use super::{LoadedCatalog, ManifestManagedRun, ManifestTask, RunnerError};
use preflight::build_execution_preflight;

pub(super) fn task_run_preview(task: &ManifestTask) -> String {
    if let Some(run) = task.run.as_ref() {
        return match run {
            ManifestManagedRun::Command(command) => command.clone(),
            ManifestManagedRun::Sequence(steps) => format!("<sequence:{}>", steps.len()),
        };
    }
    if let Some(mode) = task.mode.as_ref() {
        return format!("<managed:{mode}>");
    }
    "<none>".to_owned()
}

pub(super) fn catalog_task_label(catalog: &LoadedCatalog, task_name: &str) -> String {
    if catalog.depth == 0 {
        task_name.to_owned()
    } else {
        format!("{}/{}", catalog.alias, task_name)
    }
}

pub(super) fn run_manifest_task(task: &TaskInvocation) -> Result<String, RunnerError> {
    let cwd = std::env::current_dir().map_err(RunnerError::Cwd)?;
    run_manifest_task_with_cwd(task, cwd)
}

pub(super) fn run_manifest_task_with_cwd(
    task: &TaskInvocation,
    cwd: PathBuf,
) -> Result<String, RunnerError> {
    let preflight = build_execution_preflight(task, cwd)?;
    pipeline::run_execution_pipeline(task, preflight)
}
