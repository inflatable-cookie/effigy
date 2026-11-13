use effigy_containers::exec::ContainerExecError;

/// A capability this environment was expected to provide.
///
/// A missing capability is an environment fact: the request was not wrong, and
/// retrying it unchanged asks the same question again. It is never reported as
/// a refused invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// The container engine (Docker, Colima, nerdctl) could not be observed.
    ContainerRuntime,
    /// The container offers no shell this runtime can hand a task to.
    ContainerShell,
}

impl Capability {
    /// The name a person reads and the runner's classification share.
    pub fn label(self) -> &'static str {
        match self {
            Self::ContainerRuntime => "container runtime",
            Self::ContainerShell => "container shell",
        }
    }
}

#[derive(Debug)]
pub enum EffigyRuntimeError {
    Cwd(std::io::Error),
    Ui(String),
    TaskInvocation(String),
    /// The environment cannot supply `capability`; `detail` carries the
    /// observation that refused. Kept apart from [`Self::TaskInvocation`], which
    /// is a refusal a caller can fix by changing the request.
    Unavailable {
        capability: Capability,
        detail: String,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
}

impl EffigyRuntimeError {
    pub fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    pub fn unavailable(capability: Capability, detail: impl Into<String>) -> Self {
        Self::Unavailable {
            capability,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for EffigyRuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cwd(error) => write!(f, "{error}"),
            Self::Ui(message) | Self::TaskInvocation(message) => {
                write!(f, "{message}")
            }
            Self::Unavailable { capability, detail } => {
                write!(f, "{} unavailable: {detail}", capability.label())
            }
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
            // No child ended, so no exit code or signal is claimed: the
            // observation refused its own evidence and says why.
            ContainerExecError::Refusal { command, reason } => {
                Self::task_invocation(format!("{command} refused: {reason}"))
            }
            ContainerExecError::Failure {
                command,
                code,
                stdout,
                stderr,
            } => Self::task_invocation(format!(
                "{command} failed ({})\nstdout:\n{}\nstderr:\n{}",
                effigy_ui::human::exit_code(code),
                stdout,
                stderr
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Capability, EffigyRuntimeError};
    use effigy_containers::exec::ContainerExecError;

    /// A missing capability says which one, and says it once: the runner reports
    /// this sentence instead of inventing a command that failed.
    #[test]
    fn a_missing_capability_names_itself() {
        let error =
            EffigyRuntimeError::unavailable(Capability::ContainerRuntime, "engine not found");
        assert_eq!(
            error.to_string(),
            "container runtime unavailable: engine not found"
        );
    }

    /// A refusal keeps its own class: the command said no, so this is not an
    /// unavailable environment.
    #[test]
    fn a_refused_container_command_stays_a_refusal() {
        let error = EffigyRuntimeError::from(ContainerExecError::Refusal {
            command: "docker ps".to_owned(),
            reason: "the daemon is not running".to_owned(),
        });
        assert!(matches!(error, EffigyRuntimeError::TaskInvocation(_)));
    }
}
