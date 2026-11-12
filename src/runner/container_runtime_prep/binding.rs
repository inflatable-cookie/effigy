//! The owned binding of one running service: the container this checkout and
//! scope own, proven by inspecting it, for the stages that need a full id.

use std::path::Path;
use std::time::{Duration, Instant};

// Inventory and strict inspection require separate runtime transport hops. They
// must not consume the short workload probe's execution window. Explicit caller
// deadlines still bound both phases without being extended.
pub(in crate::runner) const OWNED_EXEC_VERIFICATION_TIMEOUT: Duration = Duration::from_secs(15);

use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

/// Verify the owned binding of `service`: the running container that this
/// checkout and scope own. Readiness itself is Running plus the service's own
/// health check, which the bring-up wait already decided (C1); an exec that
/// cannot run in the container reports its own error.
pub(in crate::runner) fn verify_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Option<Instant>,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    verified_service_binding(
        repo_root,
        policy,
        service,
        deadline.unwrap_or_else(|| Instant::now() + OWNED_EXEC_VERIFICATION_TIMEOUT),
    )
}

/// The owned binding of the environment's primary service, for the stages that
/// need an inspected full id (mount verification, alias reconciliation).
/// `deadline` is the bring-up's observation budget when the caller shares one;
/// the standard inventory budget applies when it does not.
pub(in crate::runner) fn ensure_primary_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    verify_service_binding(repo_root, policy, &policy.primary_service, deadline)
}

pub(super) fn verified_service_binding(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    service: &str,
    deadline: Instant,
) -> Result<effigy_containers::exec::VerifiedComposeContainer, RunnerError> {
    let rows = effigy_containers::exec::list_running_compose_containers_for_policy_with_deadline(
        repo_root,
        policy,
        Some(deadline),
    )?;
    // A selected-service operation must not probe or mutate its siblings.
    let selected = rows
        .into_iter()
        .filter(|row| row.service.as_deref() == Some(service))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        let reason = match crate::runner::environment::record(policy) {
            Some(record) if record.state == effigy_containers::environment::EnvState::Stopped => {
                format!(
                    "environment `{}` was stopped before this command could start",
                    policy.name
                )
            }
            Some(record)
                if record
                    .engine()
                    .is_some_and(|engine| engine.project != policy.project_name) =>
            {
                format!(
                    "resolved project `{}` differs from the environment's recorded project `{}`",
                    policy.project_name,
                    record.engine().expect("checked engine").project
                )
            }
            _ => format!(
                "the runtime reported no running target in resolved project `{}`",
                policy.project_name
            ),
        };
        return Err(RunnerError::task_invocation(format!(
            "container service `{service}` is not running: {reason}; inspect it with `effigy container {} status`, then start it with `effigy container {} up`",
            policy.name, policy.name
        )));
    }
    let mut bindings = effigy_containers::exec::verify_owned_running_compose_identities(
        repo_root, policy, &selected, deadline,
    )?;
    if bindings.len() != 1 {
        return Err(RunnerError::task_invocation(
            "binding unavailable: expected one owned running service",
        ));
    }
    Ok(bindings.remove(0))
}
