use crate::ManagedError;

pub struct ManagedRefContext {
    pub managed_task_name: String,
    pub process_name: String,
    pub task_ref: String,
}

impl ManagedRefContext {
    pub fn invalid(&self, detail: impl ToString) -> ManagedError {
        ManagedError::TaskManagedTaskReferenceInvalid {
            task: self.managed_task_name.clone(),
            process: self.process_name.clone(),
            reference: self.task_ref.clone(),
            detail: detail.to_string(),
        }
    }
}

pub struct StepRefContext {
    pub task_name: String,
    pub task_ref: String,
}

impl StepRefContext {
    pub fn failure(&self, detail: impl ToString) -> ManagedError {
        ManagedError::task_invocation(format!(
            "task `{}` run step task ref `{}` failed: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }

    pub fn invalid(&self, detail: impl ToString) -> ManagedError {
        ManagedError::task_invocation(format!(
            "task `{}` run step task ref `{}` is invalid: {}",
            self.task_name,
            self.task_ref,
            detail.to_string()
        ))
    }
}
