use crate::runner::error::RunnerError;
use effigy_execution::ExecutionRuntimeArgsPlan;
use effigy_tasks::TaskRuntimeArgs;

pub(super) fn prepare_execution_runtime_args(
    args: &[String],
) -> Result<(TaskRuntimeArgs, TaskRuntimeArgs, bool), RunnerError> {
    let plan = ExecutionRuntimeArgsPlan::from_args(args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    Ok((
        plan.raw_task_runtime_args(),
        plan.exec_task_runtime_args(),
        plan.output_json,
    ))
}
