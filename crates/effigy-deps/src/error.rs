use std::fmt;
use std::io;
use std::path::PathBuf;

#[derive(Debug)]
pub enum DepsError {
    Io {
        operation: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    Json {
        operation: &'static str,
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedSchema {
        path: PathBuf,
        expected_schema: &'static str,
        expected_version: u32,
        actual_schema: String,
        actual_version: u32,
    },
    InvalidState {
        path: PathBuf,
        reason: String,
    },
    RegistrationConflict {
        package_name: String,
        existing_path: PathBuf,
        requested_path: PathBuf,
    },
    LockHeld {
        path: PathBuf,
        owner_pid: Option<u32>,
    },
    Clock {
        operation: &'static str,
    },
    ProcessSpawn {
        program: String,
        cwd: PathBuf,
        source: io::Error,
    },
    ProcessFailed {
        program: String,
        cwd: PathBuf,
        status: Option<i32>,
        stderr: String,
    },
}

impl DepsError {
    pub(crate) fn io(operation: &'static str, path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::Io {
            operation,
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: serde_json::Error,
    ) -> Self {
        Self::Json {
            operation,
            path: path.into(),
            source,
        }
    }

    pub(crate) fn invalid(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::InvalidState {
            path: path.into(),
            reason: reason.into(),
        }
    }
}

impl fmt::Display for DepsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} `{}`: {source}",
                path.display()
            ),
            Self::Json {
                operation,
                path,
                source,
            } => write!(
                formatter,
                "failed to {operation} JSON `{}`: {source}",
                path.display()
            ),
            Self::UnsupportedSchema {
                path,
                expected_schema,
                expected_version,
                actual_schema,
                actual_version,
            } => write!(
                formatter,
                "unsupported dependency state schema in `{}`: expected {expected_schema} v{expected_version}, found {actual_schema} v{actual_version}",
                path.display()
            ),
            Self::InvalidState { path, reason } => {
                write!(formatter, "invalid dependency state `{}`: {reason}", path.display())
            }
            Self::RegistrationConflict {
                package_name,
                existing_path,
                requested_path,
            } => write!(
                formatter,
                "Bun registration `{package_name}` already points to `{}`; refusing requested path `{}`",
                existing_path.display(),
                requested_path.display()
            ),
            Self::LockHeld { path, owner_pid } => {
                write!(formatter, "dependency state lock `{}` is held", path.display())?;
                if let Some(owner_pid) = owner_pid {
                    write!(formatter, " by process {owner_pid}")?;
                }
                Ok(())
            }
            Self::Clock { operation } => {
                write!(formatter, "system clock is unavailable while trying to {operation}")
            }
            Self::ProcessSpawn {
                program,
                cwd,
                source,
            } => write!(
                formatter,
                "failed to run `{program}` dependency process in `{}`: {source}",
                cwd.display()
            ),
            Self::ProcessFailed {
                program,
                cwd,
                status,
                stderr,
            } => {
                write!(
                    formatter,
                    "`{program}` dependency process failed in `{}`",
                    cwd.display()
                )?;
                if let Some(status) = status {
                    write!(formatter, " with exit status {status}")?;
                }
                let stderr = stderr.trim();
                if !stderr.is_empty() {
                    write!(formatter, ": {stderr}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for DepsError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } | Self::ProcessSpawn { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}
