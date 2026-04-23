use effigy_containers::exec::ContainerExecError;

#[derive(Debug)]
pub enum EffigyRuntimeError {
    Cwd(std::io::Error),
    Ui(String),
    TaskInvocation(String),
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
}

impl EffigyRuntimeError {
    pub fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }
}

impl std::fmt::Display for EffigyRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cwd(error) => write!(f, "{error}"),
            Self::Ui(message) | Self::TaskInvocation(message) => write!(f, "{message}"),
            Self::TaskCommandLaunch { command, error } => {
                write!(f, "failed to launch `{command}`: {error}")
            }
        }
    }
}

impl std::error::Error for EffigyRuntimeError {}

impl From<ContainerExecError> for EffigyRuntimeError {
    fn from(value: ContainerExecError) -> Self {
        match value {
            ContainerExecError::Launch { command, error } => {
                Self::TaskCommandLaunch { command, error }
            }
            ContainerExecError::Failure {
                command,
                code,
                stdout,
                stderr,
            } => Self::task_invocation(format!(
                "{command} failed (code {:?})\nstdout:\n{}\nstderr:\n{}",
                code, stdout, stderr
            )),
        }
    }
}
