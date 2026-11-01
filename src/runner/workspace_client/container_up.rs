//! Supervisor-local container activation. Only command intent crosses the
//! process boundary; a render seal, reservation, backend handle or deadline
//! must never be serialized and reconstructed by the front end.

use std::time::{Duration, Instant};

use effigy_containers::environment::Retention;
use effigy_kernel::run::RunError;
use serde::{Deserialize, Serialize};

use super::serving_host::ServingHostRequest;
use super::supervisor::ServiceOwner;

/// The front end's intent. No credential or purported observation is accepted.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContainerUpRequest {
    pub(crate) environment: String,
    pub(crate) retention: Retention,
    pub(crate) budget_ms: u64,
}

impl ContainerUpRequest {
    fn deadline(&self, host: &ServingHostRequest) -> Result<Instant, RunError> {
        if self.environment.is_empty() || self.environment != host.environment {
            return Err(RunError::refused(
                "workspace activation environment differs from its host",
            ));
        }
        if self.retention != Retention::Persistent {
            return Err(RunError::refused(
                "held workspace activation has no detached holder handoff",
            ));
        }
        if !host.project_path.is_absolute() || !host.state_home.is_absolute() {
            return Err(RunError::refused(
                "workspace activation requires absolute checkout and state paths",
            ));
        }
        if self.budget_ms == 0 {
            return Err(RunError::refused("workspace activation budget is empty"));
        }
        Instant::now()
            .checked_add(Duration::from_millis(self.budget_ms))
            .ok_or_else(|| RunError::refused("workspace activation budget overflows its deadline"))
    }
}

/// Observe identity before any preparation or mount/secret effect. The worker
/// is read-only, so timeout cannot leave an abandoned mutator behind.
fn validate_identity(host: &ServingHostRequest, until: Instant) -> Result<(), RunError> {
    use crate::runner::workspace_command::provisioning::bounded_publication_read;
    let host = host.clone();
    bounded_publication_read("workspace-startup-identity", until, move || {
        let unavailable = || {
            crate::runner::error::RunnerError::task_invocation(
                "workspace checkout/backend identity observation unavailable",
            )
        };
        let token = effigy_core::worktree_scope::checkout_token(&host.project_path)
            .map_err(|_| unavailable())?
            .ok_or_else(unavailable)?;
        let expected: String = host
            .checkout
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if token != expected {
            return Err(crate::runner::error::RunnerError::task_invocation(
                "workspace activation checkout differs from its host",
            ));
        }
        let kind = match effigy_core::worktree_scope::scope_kind(&host.project_path)
            .map_err(|_| unavailable())?
        {
            Some(effigy_core::worktree_scope::ScopeKind::LinkedWorktree) => {
                effigy_kernel::id::CheckoutKind::LinkedWorktree
            }
            Some(effigy_core::worktree_scope::ScopeKind::EphemeralClone) => {
                effigy_kernel::id::CheckoutKind::EphemeralClone
            }
            None => effigy_kernel::id::CheckoutKind::Primary,
        };
        let environment = effigy_containers::environment::EnvRef::new(
            effigy_kernel::id::CheckoutId::new(kind, host.checkout),
            &host.environment,
        )
        .map_err(|_| unavailable())?;
        let observe = crate::runner::task_engine::observe_context(
            until.saturating_duration_since(Instant::now()),
        );
        let location = effigy_kernel::location::CheckoutLocation {
            root: effigy_core::worktree_scope::checkout_root(&host.project_path),
        };
        let root_observed = effigy_kernel::root::StateRoot::checkout_observe(&observe, &location);
        let root = root_observed.verified().ok_or_else(unavailable)?;
        let record_observed =
            effigy_containers::environment::EnvRecord::read(&observe, root, &environment.resource);
        let record = record_observed.verified().ok_or_else(unavailable)?;
        let selected = record.recorded_backend().ok_or_else(unavailable)?;
        if selected.context != host.profile {
            return Err(crate::runner::error::RunnerError::task_invocation(
                "workspace activation backend profile differs from its host",
            ));
        }
        Ok(())
    })
    .map_err(|error| RunError::unavailable(error.to_string()))
}

/// Refuse unsupported Phase-A custody before invoking mutating preparation.
/// No plan, seal or run is reconstructed, and no ordinary reconcile fallback
/// can turn this missing capability into an activation acknowledgement.
pub(crate) fn activate(
    host: &ServingHostRequest,
    request: ContainerUpRequest,
) -> Result<ServiceOwner<'static>, RunError> {
    let until = request.deadline(host)?;
    validate_identity(host, until)?;
    Err(RunError::unavailable(
        "bounded workspace Phase-A preparation is unavailable; no preparation or apply was attempted",
    ))
}

#[cfg(test)]
#[path = "container_up_tests.rs"]
mod tests;
