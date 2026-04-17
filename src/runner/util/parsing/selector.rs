use effigy_tasks::TaskSelector;

use crate::runner::error::RunnerError;

pub(super) fn parse_task_selector(raw: &str) -> Result<TaskSelector, RunnerError> {
    effigy_tasks::parse_task_selector(raw).map_err(RunnerError::task_invocation)
}
