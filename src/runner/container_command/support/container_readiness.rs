//! Waiting for a container environment to be ready.
//!
//! The environment's services are awaited by `readiness`; this module adds the
//! declared `[containers.<name>.health]` gate, waits for it under the identity
//! the runtime reported, and turns an unfinished wait into a refusal that names
//! the wait it was in (effigy#455, effigy#460, Q-017).

use effigy_containers::exec::{
    container_last_log_line_for_policy, list_running_compose_containers_for_policy_with_deadline,
};
use effigy_containers::health::{wait_for_ready, HealthWaitError};
use effigy_containers::EffectiveContainerPolicy;
use effigy_ui::steps::{ProgressFrame, ProgressSource};
use serde_json::json;

use crate::runner::container_command::RunnerError;

/// Wait until the environment is ready, naming each service as it waits.
///
/// Every service's own health check decides first (effigy#313), because that
/// is what tells a person which service is wrong. The environment's declared
/// `[containers.<name>.health]` check is still the final gate when it declares
/// one; an environment whose services the runtime cannot see keeps exactly
/// the old behaviour. `deadline` is the bring-up's one observation budget; a
/// caller without one starts the environment's own health timeout now.
pub(in crate::runner::container_command) fn wait_for_container_ready(
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&std::sync::atomic::AtomicBool>,
    deadline: Option<std::time::Instant>,
    waits: crate::runner::container_command::readiness::PhaseObserver<'_>,
) -> Result<Option<&'static str>, RunnerError> {
    let readiness = crate::runner::container_command::readiness::await_services_ready(
        &policy.repo_root,
        policy,
        stop_requested,
        deadline,
        waits,
    )?;
    match readiness.end {
        crate::runner::container_command::readiness::ReadinessEnd::Interrupted => {
            return Ok(Some("interrupted"))
        }
        crate::runner::container_command::readiness::ReadinessEnd::Ready
        | crate::runner::container_command::readiness::ReadinessEnd::NothingToAwait => {}
    }
    let Some(check) = policy.health_check.as_deref() else {
        return Ok(None);
    };
    // The declared check probes one service's endpoint, and the container that
    // serves it is the identity the runtime reported for that service: the
    // readiness probe already observed it, so it is carried from there. When it
    // was never observed, one live step covers the runtime read that names it,
    // so a stalled read is never an idle command (effigy#460, Q-017).
    let container = readiness
        .observed
        .iter()
        .find(|service| service.service == policy.primary_service)
        .map(|service| service.container.clone())
        .or_else(|| primary_service_container_observed(policy, deadline));
    let subject = match &container {
        Some(container) => format!("container `{container}`"),
        None => format!(
            "environment `{}` (primary service `{}`)",
            policy.name, policy.primary_service
        ),
    };
    wait_for_ready(
        &subject,
        Some(check),
        policy.health_timeout_secs,
        stop_requested,
    )
    .map_err(|error| declared_health_refusal(policy, error, container))
}

/// The container the primary service actually runs as: the runtime's own row
/// for that Compose service. Compose may declare an explicit `container_name`
/// and a runtime may name it differently, so Effigy never assumes the
/// `<project>-<service>-1` default for an identity it can read (effigy#460).
///
/// `deadline` is the bring-up's one observation budget, so this read never
/// grants the runtime a fresh one. The read runs under its own live step: the
/// readiness wait has already finished, and a runtime that stops answering here
/// must still say what it is waiting for (Q-017).
pub(super) fn primary_service_container_observed(
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Option<String> {
    let progress = primary_container_progress(policy);
    let step = crate::runner::container_command::steps::TimedStep::live(progress);
    let container = read_primary_service_container(policy, deadline);
    match &container {
        Some(container) => step.succeed(format!(
            "primary service `{}` runs as `{container}`",
            policy.primary_service
        )),
        // The declared wait names the environment and service instead; this
        // read's own failure is not the reason the check failed.
        None => step.finish_clear(),
    }
    container
}

/// The exact wait the identity read is in: the runtime's row for the primary
/// service, with elapsed time (effigy#460, Q-017).
pub(super) fn primary_container_progress(policy: &EffectiveContainerPolicy) -> ProgressSource {
    ProgressSource::new(
        ProgressFrame::new(format!(
            "reading the runtime's row for primary service `{}` of `{}`",
            policy.primary_service, policy.project_name
        ))
        .label(format!("Checking {} container", policy.primary_service))
        .waiting_for(format!(
            "the runtime to name the primary container of `{}`",
            policy.project_name
        )),
    )
}

/// One runtime inventory read, selecting the primary service's own row.
fn read_primary_service_container(
    policy: &EffectiveContainerPolicy,
    deadline: Option<std::time::Instant>,
) -> Option<String> {
    let rows = list_running_compose_containers_for_policy_with_deadline(
        &policy.repo_root,
        policy,
        deadline,
    )
    .ok()?;
    rows.into_iter()
        .find(|row| row.service.as_deref() == Some(policy.primary_service.as_str()))
        .map(|row| row.container_name)
}

/// How many trailing log lines the failed container is read for.
const DECLARED_HEALTH_LOG_TAIL: u32 = 20;

/// The last line the container itself printed, read only when a declared check
/// did not come ready: the evidence its health check cannot give (effigy#455).
///
/// The read is a bounded runtime call of its own (up to 30s), and the declared
/// check's step has already ended, so it runs under a live step that names the
/// container it reads and reports when the runtime does not answer
/// (effigy#460, Q-017).
pub(super) fn declared_health_tail(
    policy: &EffectiveContainerPolicy,
    container: &str,
) -> Option<String> {
    let step = crate::runner::container_command::steps::TimedStep::live(
        declared_health_tail_progress(container),
    );
    match container_last_log_line_for_policy(
        &policy.repo_root,
        policy,
        container,
        DECLARED_HEALTH_LOG_TAIL,
    ) {
        Ok(line) if !line.is_empty() => {
            step.succeed(format!("read the last line `{container}` printed"));
            Some(line)
        }
        // The container printed nothing this read kept; the refusal still
        // carries the status it observed.
        Ok(_) => {
            step.finish_clear();
            None
        }
        Err(error) => {
            step.fail(format!(
                "the runtime did not answer the log read for `{container}`: {}",
                first_line(&error.to_string())
            ));
            None
        }
    }
}

/// The exact wait the log tail is in: the runtime's own log read for the
/// container that failed, with elapsed time (effigy#460, Q-017).
pub(super) fn declared_health_tail_progress(container: &str) -> ProgressSource {
    ProgressSource::new(
        ProgressFrame::new(format!("reading the last line `{container}` printed"))
            .label("Reading container logs")
            .waiting_for(format!("the runtime's log tail of `{container}`")),
    )
}

/// The first line of a runtime error, bounded so one frame stays one line.
fn first_line(text: &str) -> String {
    const LIMIT: usize = 160;
    let line = text.lines().next().unwrap_or_default();
    if line.chars().count() <= LIMIT {
        return line.to_owned();
    }
    let cut: String = line.chars().take(LIMIT).collect();
    format!("{cut}…")
}

/// A declared health check that did not turn ready, as the wait it was in: the
/// container, the check, the answer it gave and the container's own last
/// output (effigy#460, Q-017). A definite answer is terminal, so the refusal
/// names the status it observed instead of a spent budget (effigy#455).
pub(super) fn declared_health_refusal(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
    container: Option<String>,
) -> RunnerError {
    let tail = container
        .as_deref()
        .and_then(|container| declared_health_tail(policy, container));
    declared_health_refusal_with_evidence(policy, error, container, tail)
}

/// The same refusal with the container and its last output already read, so the
/// message and its machine-readable waits are testable without a runtime.
pub(super) fn declared_health_refusal_with_evidence(
    policy: &EffectiveContainerPolicy,
    error: HealthWaitError,
    container: Option<String>,
    tail: Option<String>,
) -> RunnerError {
    let mut message = error.to_string();
    if let Some(tail) = &tail {
        message.push_str(&format!("; last container output: {tail}"));
    }
    let mut wait = json!({
        "kind": "health_check",
        "environment": policy.name,
        "service": policy.primary_service,
        "container": container,
        "check": error.check(),
        "timeout_secs": error.timeout_secs(),
        "probe": "declared health check",
        "answer": error.outcome(),
    });
    if let Some(status) = error.observed_status() {
        wait["status"] = json!(status);
    }
    if let Some(tail) = &tail {
        wait["last_output"] = json!(tail);
    }
    RunnerError::container_wait_refused(
        &policy.name,
        message,
        json!({
            "failure_kind": "container_readiness",
            "readiness": "unverified",
            "waiting_on": [wait],
        }),
    )
}
