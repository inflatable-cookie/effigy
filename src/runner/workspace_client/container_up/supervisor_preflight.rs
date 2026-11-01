//! Detached Phase 0: independently rebuild declarative facts before opening
//! any effects root. The returned policy/descriptor never crosses a process.
use std::time::Instant;

use effigy_containers::environment::BackendSelection;
use effigy_containers::EffectiveContainerPolicy;
use effigy_kernel::fs::DirFd;
use effigy_kernel::run::RunError;

use super::cli_preflight::{read_backend, read_checkout};
use super::{ContainerUpRequest, ServingHostRequest};
use crate::runner::workspace_command::provisioning::{
    bounded_publication_read, workspace_path_map, workspace_source_closure,
};

pub(super) struct PrevalidatedContainer {
    pub(super) policy: EffectiveContainerPolicy,
    pub(super) backend: BackendSelection,
    pub(super) until: Instant,
    directory: DirFd,
}

impl PrevalidatedContainer {
    /// Keep the original read descriptor alive until the supervisor builds its
    /// retained plan; a replacement at the same pathname cannot inherit it.
    pub(super) fn recheck_directory(&self) -> Result<(), RunError> {
        let current = DirFd::open(&self.policy.repo_root)
            .and_then(|directory| directory.identity())
            .map_err(|_| unavailable())?;
        if self.directory.identity().map_err(|_| unavailable())? != current {
            return Err(RunError::refused(
                "workspace checkout directory changed before sealing",
            ));
        }
        if Instant::now() >= self.until {
            return Err(RunError::unavailable(
                "workspace activation budget exhausted",
            ));
        }
        Ok(())
    }
}

fn unavailable() -> RunError {
    RunError::unavailable("workspace post-prep identity observation unavailable")
}

pub(super) fn validate(
    host: &ServingHostRequest,
    request: &ContainerUpRequest,
    until: Instant,
) -> Result<PrevalidatedContainer, RunError> {
    let declaration = request.post_prep.clone().ok_or_else(|| {
        RunError::refused("workspace activation requires a post-prep declaration")
    })?;
    let host = host.clone();
    // This worker is strictly read-only. Expiry may abandon a read, never a
    // mutating preparation/apply operation.
    bounded_publication_read("workspace-post-prep-validation", until, move || {
        let refuse = |message| crate::runner::error::RunnerError::task_invocation(message);
        let checkout = read_checkout(&host.project_path)?
            .ok_or_else(|| refuse("workspace checkout identity is absent"))?;
        if *checkout.token() != host.checkout {
            return Err(refuse(
                "workspace activation checkout differs from its host",
            ));
        }
        let directory = DirFd::open(&host.project_path)
            .map_err(|_| refuse("workspace checkout identity observation unavailable"))?;
        let identity = directory
            .identity()
            .map_err(|_| refuse("workspace checkout identity observation unavailable"))?;
        let policy =
            effigy_containers::load_container_policy(&host.project_path, Some(&host.environment))
                .map_err(|_| refuse("workspace post-prep policy observation unavailable"))?;
        let workspace = crate::runner::workspace_command::operator_workspace_for_policy(
            &host.project_path,
            &policy,
        )
        .map_err(|_| refuse("workspace declaration observation unavailable"))?
        .ok_or_else(|| refuse("workspace declaration changed after preparation"))?;
        let backend = read_backend(&policy, Some(checkout))?;
        if backend.context != host.profile {
            return Err(refuse(
                "workspace activation backend profile differs from its host",
            ));
        }
        // Verification failures contain only fixed classifications. Never emit
        // the policy, backend stderr or source bytes through the control pipe.
        declaration
            .verify_policy(&policy, &backend, identity)
            .map_err(|error| {
                crate::runner::error::RunnerError::task_invocation(error.to_string())
            })?;
        let paths = workspace_path_map(&host.project_path, &workspace)
            .map_err(|_| refuse("workspace post-prep source observation unavailable"))?;
        let sources = workspace_source_closure(&host.project_path, &paths)
            .map_err(|_| refuse("workspace post-prep source observation unavailable"))?;
        declaration.verify_sources(&sources.pins).map_err(|error| {
            crate::runner::error::RunnerError::task_invocation(error.to_string())
        })?;
        Ok(PrevalidatedContainer {
            policy,
            backend,
            until,
            directory,
        })
    })
    .map_err(|error| RunError::unavailable(error.to_string()))
}

#[cfg(test)]
#[path = "supervisor_preflight_tests.rs"]
mod tests;
