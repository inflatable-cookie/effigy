//! Error types for the exec crate.

use std::path::PathBuf;

/// Errors that can occur during exec operations.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// The host working directory is outside the repo root.
    #[error(
        "working directory {cwd} is outside repo root {repo_root} \
         — cannot map to container path"
    )]
    CwdOutsideRepo { cwd: PathBuf, repo_root: PathBuf },

    /// The exec alias was not found.
    #[error("exec alias '{name}' not found (available: {available:?})")]
    AliasNotFound {
        name: String,
        available: Vec<String>,
    },

    /// The container is not running.
    #[error(
        "container '{container}' is not running — start it with \
         `effigy container up` or `effigy dev`"
    )]
    ContainerNotRunning { container: String },

    /// Multiple containers claim the dev context.
    #[error(
        "multiple containers claim context = \"dev\": {containers:?} \
         — only one container per project may be the dev context"
    )]
    AmbiguousContext { containers: Vec<String> },
}
