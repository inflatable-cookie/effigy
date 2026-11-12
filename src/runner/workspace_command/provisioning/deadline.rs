//! Bound read-only publication inputs by the publication deadline.

use super::artifact_acquisition::held_linux_workspace_effigy_artifact;
use super::artifact_plan::LinuxWorkspaceTarget;
use super::artifact_receipt::verify_guest_artifact_until;
use super::sources::{operator_workspace_for_policy, OperatorWorkspace};
use crate::runner::workspace_client::context::GuestBinaryIdentity;
use crate::runner::RunnerError;
use effigy_containers::EffectiveContainerPolicy;
use std::path::PathBuf;
/// Why one deadline-bounded publication read produced no value.
#[derive(Debug)]
pub(in crate::runner) enum BoundedReadRefusal {
    /// The publication's one absolute deadline passed before the read finished.
    TimedOut,
    /// The read finished but could not produce a value.
    Unavailable(String),
}

impl std::fmt::Display for BoundedReadRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut => {
                formatter.write_str("the publication deadline expired during the read")
            }
            Self::Unavailable(reason) => formatter.write_str(reason),
        }
    }
}

/// Bound synchronous read-only input work by the absolute publication deadline.
/// A blocked filesystem read cannot be interrupted, so a worker performs it
/// while the caller stops waiting and reports unknown. An abandoned worker
/// cannot publish, mutate or hold authority.
pub(in crate::runner) fn bounded_publication_read<T, F>(
    name: &'static str,
    until: std::time::Instant,
    read: F,
) -> Result<T, BoundedReadRefusal>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, RunnerError> + Send + 'static,
{
    use std::sync::mpsc::{channel, RecvTimeoutError};

    let (send, receive) = channel();
    let worker = std::thread::Builder::new()
        .name(format!("effigy-publication-{name}"))
        .spawn(move || {
            let _ = send.send(read());
        })
        .map_err(|error| {
            BoundedReadRefusal::Unavailable(format!("the {name} worker could not start: {error}"))
        })?;
    let remaining = until.saturating_duration_since(std::time::Instant::now());
    let outcome = if remaining.is_zero() {
        Err(RecvTimeoutError::Timeout)
    } else {
        receive.recv_timeout(remaining)
    };
    match outcome {
        Ok(Ok(value)) => {
            let _ = worker.join();
            Ok(value)
        }
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(BoundedReadRefusal::Unavailable(error.to_string()))
        }
        Err(RecvTimeoutError::Timeout) => Err(BoundedReadRefusal::TimedOut),
        Err(RecvTimeoutError::Disconnected) => {
            let _ = worker.join();
            Err(BoundedReadRefusal::Unavailable(format!(
                "the {name} worker ended without a result"
            )))
        }
    }
}

/// Resolve and compose the declared workspace within the publication deadline.
pub(in crate::runner) fn operator_workspace_until(
    repo_root: PathBuf,
    policy: EffectiveContainerPolicy,
    until: std::time::Instant,
) -> Result<Option<OperatorWorkspace>, BoundedReadRefusal> {
    bounded_publication_read("operator-workspace", until, move || {
        operator_workspace_for_policy(repo_root.as_path(), &policy)
    })
}

/// Find a produced artifact and read its sidecars within the publication deadline.
/// This step never builds or downloads.
pub(in crate::runner) fn held_linux_workspace_effigy_artifact_until(
    repo_root: PathBuf,
    target: LinuxWorkspaceTarget,
    until: std::time::Instant,
) -> Result<PathBuf, BoundedReadRefusal> {
    bounded_publication_read("workspace-artifact", until, move || {
        held_linux_workspace_effigy_artifact(repo_root.as_path(), target)
    })
}

/// Authenticate the version, receipt and artifact digest in one bounded read.
pub(in crate::runner) fn verify_guest_artifact_bounded(
    artifact: PathBuf,
    target: LinuxWorkspaceTarget,
    until: std::time::Instant,
) -> Result<GuestBinaryIdentity, BoundedReadRefusal> {
    bounded_publication_read("workspace-artifact-receipt", until, move || {
        verify_guest_artifact_until(artifact.as_path(), target, Some(until))
    })
}
