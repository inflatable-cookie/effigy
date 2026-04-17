use effigy_tasks::TaskRuntimeArgs;

use crate::runner::error::RunnerError;

pub(super) fn parse_task_runtime_args(args: &[String]) -> Result<TaskRuntimeArgs, RunnerError> {
    effigy_tasks::parse_task_runtime_args(args).map_err(RunnerError::task_invocation)
}
