use super::super::super::RunnerError;

pub(super) struct ManagedRefContext {
    pub(super) managed_task_name: String,
    pub(super) process_name: String,
    pub(super) task_ref: String,
}

impl ManagedRefContext {
    pub(super) fn invalid(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskManagedTaskReferenceInvalid {
            task: self.managed_task_name.clone(),
            process: self.process_name.clone(),
            reference: self.task_ref.clone(),
            detail: detail.to_string(),
        }
    }
}

pub(super) struct StepRefContext {
    pub(super) task_name: String,
    pub(super) task_ref: String,
}

impl StepRefContext {
    pub(super) fn failure(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskInvocation(format!(
            "task `{}` run step task ref `{}` failed: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }

    pub(super) fn invalid(&self, detail: impl ToString) -> RunnerError {
        RunnerError::TaskInvocation(format!(
            "task `{}` run step task ref `{}` is invalid: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }
}
