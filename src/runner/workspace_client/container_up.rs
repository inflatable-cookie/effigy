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

/// Runs inside the detached serving process. The retained plan contains the
/// exact preflight seal and pins its backend and single absolute deadline.
/// Until the parked integration adapter is available, refuse before apply;
/// the ordinary reconciler is not an alternate activation owner.
pub(crate) fn activate(
    host: &ServingHostRequest,
    request: ContainerUpRequest,
) -> Result<ServiceOwner<'static>, RunError> {
    let until = request.deadline(host)?;
    let preparation = crate::runner::container_command::lifecycle::prepare_workspace_up(
        &host.project_path,
        &request.environment,
    )
    .map_err(|error| RunError::step_failed("workspace container preparation", error))?;
    // Legacy preparation helpers do not all accept a deadline yet. This
    // refuses late completion; it does not prove interruptible preparation.
    let policy = &preparation.policy;
    let environment = crate::runner::environment::env_ref(policy)
        .map_err(|error| RunError::step_failed("workspace checkout identity", error))?;
    if environment.checkout.token() != &host.checkout {
        return Err(RunError::refused(
            "workspace activation checkout differs from its host",
        ));
    }
    let deadline =
        crate::runner::container_command::reserved_input::PublicationDeadline::new(until);
    deadline
        .remaining()
        .map_err(|error| RunError::unavailable(error.to_string()))?;
    let mut backend =
        crate::runner::container_command::reserved_input::ContainerBackendObservation::prepare(
            policy,
        )
        .map_err(|error| RunError::step_failed("workspace backend observation", error))?;
    let retained = crate::runner::container_command::reserved_input::plan(
        crate::runner::container_command::reserved_input::ReservedPlanRequest { policy, deadline },
        &mut backend,
    )
    .map_err(|error| RunError::unavailable(error.to_string()))?
    .ok_or_else(|| RunError::refused("hybrid environment has no workspace activation"))?;
    retained
        .deadline()
        .remaining()
        .map_err(|error| RunError::unavailable(error.to_string()))?;
    Err(RunError::unavailable(
        "workspace parked integration adapter is unavailable; no apply was attempted",
    ))
}

#[cfg(test)]
#[path = "container_up_tests.rs"]
mod tests;
