use super::{ProcessManagerError, ResolveError, RunnerError, TaskError};

#[path = "error/display.rs"]
mod display;
#[path = "error/rendered_output.rs"]
mod rendered_output;

impl std::fmt::Display for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        display::fmt_runner_error(self, f)
    }
}

impl std::error::Error for RunnerError {}

impl RunnerError {
    pub fn rendered_output(&self) -> Option<&str> {
        rendered_output::runner_error_rendered_output(self)
    }
}

impl From<TaskError> for RunnerError {
    fn from(value: TaskError) -> Self {
        Self::Task(value)
    }
}

impl From<crate::ui::UiError> for RunnerError {
    fn from(value: crate::ui::UiError) -> Self {
        Self::Ui(value.to_string())
    }
}

impl From<ResolveError> for RunnerError {
    fn from(value: ResolveError) -> Self {
        Self::Resolve(value)
    }
}

impl From<ProcessManagerError> for RunnerError {
    fn from(value: ProcessManagerError) -> Self {
        Self::ManagedProcess(value)
    }
}
