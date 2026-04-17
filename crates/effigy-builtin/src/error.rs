//! Narrow error boundary for the built-in command layer.
//!
//! Produced by every built-in arm (dispatcher, registry, test, scan,
//! config, watch, unlock, cache, completion, …) and lifted to
//! `RunnerError` at the runner's edge via `impl From<BuiltinError> for
//! RunnerError` in `src/runner/error.rs`. Variant shapes mirror
//! `RunnerError::*` one-for-one so the runner-side `From` impl is a
//! pure re-shape, not a lossy conversion.
//!
//! The port trait (see `ports.rs`) returns `Result<T, BuiltinError>`;
//! the runner's concrete impl converts `RunnerError` → `BuiltinError`
//! at that boundary so built-in code only speaks `BuiltinError`.

use std::path::{Path, PathBuf};

use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_managed::ManagedError;
use effigy_manifest::ManifestError;
use effigy_routing::RoutingError;
use effigy_scan::ScanError;
use effigy_ui::UiError;

#[derive(Debug)]
pub enum BuiltinError {
    TaskInvocation(String),
    Ui(String),
    TaskManifestCompose {
        path: PathBuf,
        detail: String,
    },
    TaskCommandLaunch {
        command: String,
        error: std::io::Error,
    },
    TaskLockConflict {
        scope: String,
        lock_path: PathBuf,
        holder_pid: Option<u32>,
        holder_started_at_epoch_ms: Option<u128>,
        holder_heartbeat_at_epoch_ms: Option<u128>,
        holder_hostname: Option<String>,
        holder_workspace_root: Option<String>,
        remediation: String,
    },
    TaskLockIo {
        path: PathBuf,
        error: std::io::Error,
    },
    BuiltinTestNonZero {
        failures: Vec<(String, Option<i32>)>,
        rendered: String,
    },
    BuiltinScanNonZero {
        finding_count: usize,
        rendered: String,
    },
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    Manifest(ManifestError),
    Managed(ManagedError),
    Routing(RoutingError),
    Scan(ScanError),
}

impl BuiltinError {
    pub fn task_invocation(message: impl Into<String>) -> Self {
        Self::TaskInvocation(message.into())
    }

    pub fn task_invocation_failed_read(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_read_path(path, error))
    }

    pub fn task_invocation_failed_parse(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_parse_path(path, error))
    }

    pub fn task_invocation_failed_write(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_write_path(path, error))
    }

    pub fn task_invocation_failed_render(path: &Path, error: impl std::fmt::Display) -> Self {
        Self::task_invocation(failed_to_render_path(path, error))
    }
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::TaskInvocation(message) => write!(f, "{message}"),
            BuiltinError::Ui(message) => write!(f, "{message}"),
            BuiltinError::TaskManifestCompose { path, detail } => {
                write!(f, "failed to compose manifest {}: {detail}", path.display())
            }
            BuiltinError::TaskCommandLaunch { command, error } => {
                write!(f, "failed to launch command `{command}`: {error}")
            }
            BuiltinError::TaskLockConflict { scope, .. } => {
                write!(f, "lock conflict on scope `{scope}`")
            }
            BuiltinError::TaskLockIo { path, error } => {
                write!(f, "lock io at {}: {error}", path.display())
            }
            BuiltinError::BuiltinTestNonZero { failures, .. } => {
                write!(f, "{} builtin test failure(s)", failures.len())
            }
            BuiltinError::BuiltinScanNonZero { finding_count, .. } => {
                write!(f, "{finding_count} scan finding(s)")
            }
            BuiltinError::DoctorNonZero { error_count, .. } => {
                write!(f, "doctor found {error_count} error finding(s)")
            }
            BuiltinError::Manifest(error) => write!(f, "{error}"),
            BuiltinError::Managed(error) => write!(f, "{error}"),
            BuiltinError::Routing(error) => write!(f, "{error}"),
            BuiltinError::Scan(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for BuiltinError {}

impl From<ManifestError> for BuiltinError {
    fn from(value: ManifestError) -> Self {
        BuiltinError::Manifest(value)
    }
}

impl From<ManagedError> for BuiltinError {
    fn from(value: ManagedError) -> Self {
        BuiltinError::Managed(value)
    }
}

impl From<RoutingError> for BuiltinError {
    fn from(value: RoutingError) -> Self {
        BuiltinError::Routing(value)
    }
}

impl From<ScanError> for BuiltinError {
    fn from(value: ScanError) -> Self {
        BuiltinError::Scan(value)
    }
}

impl From<UiError> for BuiltinError {
    fn from(value: UiError) -> Self {
        BuiltinError::Ui(value.to_string())
    }
}
