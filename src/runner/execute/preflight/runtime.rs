use crate::runner::error::RunnerError;
use effigy_execution::ExecutionRuntimeArgsPlan;
use effigy_tasks::TaskRuntimeArgs;

pub(super) fn prepare_execution_runtime_args(
    args: &[String],
) -> Result<(TaskRuntimeArgs, TaskRuntimeArgs, bool, bool, bool), RunnerError> {
    let plan = ExecutionRuntimeArgsPlan::from_args(args)
        .map_err(|error| RunnerError::task_invocation(error.to_string()))?;
    // Effigy's flags go before the task; everything after belongs to it.
    for flag in &plan.compat_flags {
        eprintln!(
            "note: `{flag}` after the task name is read as Effigy's flag for one more release; \
             write `effigy {flag} <task>`. After that release it is passed to the task"
        );
    }
    for flag in &plan.passed_through_flags {
        eprintln!(
            "note: `{flag}` after the task name is passed to the task; put Effigy's `{flag}` \
             before the task name"
        );
    }
    Ok((
        plan.raw_task_runtime_args(),
        plan.exec_task_runtime_args(),
        plan.output_json,
        plan.plan,
        plan.help,
    ))
}
