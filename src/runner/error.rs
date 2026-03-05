use std::path::PathBuf;

use crate::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use crate::process_manager::ProcessManagerError;
use crate::resolver::ResolveError;
use crate::tasks::TaskError;

#[path = "error/display.rs"]
mod display;
#[path = "error/rendered_output.rs"]
mod rendered_output;

#[derive(Debug)]
pub enum RunnerError {
    Cwd(std::io::Error),
    Resolve(ResolveError),
    Task(TaskError),
    Ui(String),
    TaskInvocation(String),
    TaskCatalogsMissing {
        root: PathBuf,
    },
    TaskCatalogReadDir {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestRead {
        path: PathBuf,
        error: std::io::Error,
    },
    TaskManifestParse {
        path: PathBuf,
        error: toml::de::Error,
    },
    TaskCatalogAliasConflict {
        alias: String,
        first_path: PathBuf,
        second_path: PathBuf,
    },
    TaskCatalogPrefixNotFound {
        prefix: String,
        available: Vec<String>,
    },
    TaskNotFound {
        name: String,
        path: PathBuf,
    },
    TaskNotFoundAny {
        name: String,
        catalogs: Vec<String>,
    },
    TaskAmbiguous {
        name: String,
        candidates: Vec<String>,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
    TaskCommandFailure {
        command: String,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    TaskLockConflict {
        scope: String,
        lock_path: PathBuf,
        holder_pid: Option<u32>,
        holder_started_at_epoch_ms: Option<u128>,
        remediation: String,
    },
    TaskLockIo {
        path: PathBuf,
        error: std::io::Error,
    },
    CommandJsonFailure {
        rendered: String,
    },
    ManagedProcess(ProcessManagerError),
    TaskManagedUnsupportedMode {
        task: String,
        mode: String,
    },
    TaskManagedProfileNotFound {
        task: String,
        profile: String,
        available: Vec<String>,
    },
    TaskManagedProfileEmpty {
        task: String,
        profile: String,
    },
    TaskManagedProcessNotFound {
        task: String,
        profile: String,
        process: String,
    },
    TaskManagedProcessInvalidDefinition {
        task: String,
        process: String,
        detail: String,
    },
    TaskManagedProfileTabOrderInvalid {
        task: String,
        profile: String,
        detail: String,
    },
    TaskManagedTaskReferenceInvalid {
        task: String,
        process: String,
        reference: String,
        detail: String,
    },
    TaskManagedNonZeroExit {
        task: String,
        profile: String,
        processes: Vec<(String, String)>,
    },
    TaskMissingRunCommand {
        task: String,
        path: PathBuf,
    },
    BuiltinTestNonZero {
        failures: Vec<(String, Option<i32>)>,
        rendered: String,
    },
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    DeferLoopDetected {
        depth: u8,
    },
}

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

    pub(in crate::runner) fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    pub(in crate::runner) fn task_invocation_failed_read(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_read_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_parse(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_parse_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_write(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_write_path(path, error))
    }

    pub(in crate::runner) fn task_invocation_failed_render(
        path: &std::path::Path,
        error: impl std::fmt::Display,
    ) -> Self {
        Self::task_invocation(failed_to_render_path(path, error))
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::RunnerError;

    #[test]
    fn task_invocation_constructor_preserves_message() {
        let err = RunnerError::task_invocation("message contract");
        match err {
            RunnerError::TaskInvocation(message) => assert_eq!(message, "message contract"),
            other => panic!("unexpected error variant: {other}"),
        }
    }

    #[test]
    fn task_invocation_path_message_constructors_are_stable() {
        let path = Path::new("/tmp/effigy.toml");
        let read = RunnerError::task_invocation_failed_read(path, "read-failed");
        let parse = RunnerError::task_invocation_failed_parse(path, "parse-failed");
        let write = RunnerError::task_invocation_failed_write(path, "write-failed");
        let render = RunnerError::task_invocation_failed_render(path, "render-failed");

        match read {
            RunnerError::TaskInvocation(message) => {
                assert_eq!(message, "failed to read /tmp/effigy.toml: read-failed")
            }
            other => panic!("unexpected error variant: {other}"),
        }
        match parse {
            RunnerError::TaskInvocation(message) => {
                assert_eq!(message, "failed to parse /tmp/effigy.toml: parse-failed")
            }
            other => panic!("unexpected error variant: {other}"),
        }
        match write {
            RunnerError::TaskInvocation(message) => {
                assert_eq!(message, "failed to write /tmp/effigy.toml: write-failed")
            }
            other => panic!("unexpected error variant: {other}"),
        }
        match render {
            RunnerError::TaskInvocation(message) => {
                assert_eq!(message, "failed to render /tmp/effigy.toml: render-failed")
            }
            other => panic!("unexpected error variant: {other}"),
        }
    }
}
