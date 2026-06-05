//! Narrow error boundary for the doctor orchestration layer.
//!
//! Produced by every doctor arm (workflow, checks, render, explain,
//! health, manifest scan, scan-check wrappers) and lifted to
//! `RunnerError` at the runner's edge via `impl From<DoctorError> for
//! RunnerError` in `src/runner/error.rs`. Variant shapes mirror
//! `RunnerError::*` one-for-one (and the existing `BuiltinError`
//! pattern) so the runner-side `From` impl is a pure re-shape.

use std::path::Path;

use effigy_core::path_error_text::{
    failed_to_parse_path, failed_to_read_path, failed_to_render_path, failed_to_write_path,
};
use effigy_core::resolver::ResolveError;
use effigy_manifest::ManifestError;
use effigy_scan::ScanError;
use effigy_ui::UiError;

#[derive(Debug)]
pub enum DoctorError {
    DoctorNonZero {
        error_count: usize,
        rendered: String,
    },
    TaskInvocation(String),
    Ui(String),
    CommandJsonFailure {
        rendered: String,
    },
    Manifest(ManifestError),
    Scan(ScanError),
    Routing(effigy_routing::RoutingError),
}

impl DoctorError {
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

    pub fn cwd_failure(error: std::io::Error) -> Self {
        Self::task_invocation(format!("failed to read current working directory: {error}"))
    }
}

impl std::fmt::Display for DoctorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DoctorError::DoctorNonZero { error_count, .. } => {
                write!(f, "doctor found {error_count} error finding(s)")
            }
            DoctorError::TaskInvocation(message) => write!(f, "{message}"),
            DoctorError::Ui(message) => write!(f, "{message}"),
            DoctorError::CommandJsonFailure { .. } => write!(f, "command produced non-JSON output"),
            DoctorError::Manifest(error) => write!(f, "{error}"),
            DoctorError::Scan(error) => write!(f, "{error}"),
            DoctorError::Routing(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DoctorError {}

impl From<ManifestError> for DoctorError {
    fn from(value: ManifestError) -> Self {
        DoctorError::Manifest(value)
    }
}

impl From<ScanError> for DoctorError {
    fn from(value: ScanError) -> Self {
        DoctorError::Scan(value)
    }
}

impl From<UiError> for DoctorError {
    fn from(value: UiError) -> Self {
        DoctorError::Ui(value.to_string())
    }
}

impl From<effigy_routing::RoutingError> for DoctorError {
    fn from(value: effigy_routing::RoutingError) -> Self {
        DoctorError::Routing(value)
    }
}

impl From<ResolveError> for DoctorError {
    fn from(value: ResolveError) -> Self {
        DoctorError::task_invocation(value.to_string())
    }
}
