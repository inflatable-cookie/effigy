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
