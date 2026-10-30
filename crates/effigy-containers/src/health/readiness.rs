//! Per-service readiness evidence.
//!
//! An environment's declared `[containers.<name>.health]` check proves one
//! thing about one service. A Compose stack reports readiness for every
//! service it declares, through each container's own health check, and its
//! `inspect` output says exactly why a service is not ready: the status, how
//! many checks failed in a row, and the last check output. This module turns
//! that observation into the evidence a person needs when a bring-up stops
//! short (effigy#313): which service, what its check said, and what to run.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

use crate::exec::{
    inspect_containers_for_policy,
    list_compose_containers_for_policy_including_stopped_with_deadline, ContainerExecError,
};
use crate::EffectiveContainerPolicy;

/// A failure every unready service shares: the same health check output,
/// repeated. Enough repetition means waiting cannot help.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContinuousHealthCheckFailure {
    /// The normalized check output every unready service printed.
    pub signature: String,
    /// The services that printed it, in report order.
    pub services: Vec<String>,
    /// How many consecutive observations agreed.
    pub consecutive_checks: u32,
}

/// The health a container reports for itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthState {
    Healthy,
    Unhealthy,
    Starting,
}

impl HealthState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unhealthy => "unhealthy",
            Self::Starting => "starting",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "healthy" => Some(Self::Healthy),
            "unhealthy" => Some(Self::Unhealthy),
            "starting" => Some(Self::Starting),
            _ => None,
        }
    }
}

/// One service's readiness, as the runtime reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceHealth {
    /// The Compose service name (`web`), or the container name when the
    /// runtime records no service label.
    pub service: String,
    pub container: String,
    /// `State.Status`: `running`, `exited`, `created`, `restarting`, …
    pub state: String,
    /// The container's own health check, when it declares one.
    pub health: Option<HealthState>,
    /// Consecutive failed checks. `0` when the container declares no check.
    pub failing_streak: u64,
    /// The last health check's own output, bounded to one line.
    pub last_output: Option<String>,
    /// `State.ExitCode` for a container that is not running.
    pub exit_code: Option<i64>,
    /// `HostConfig.RestartPolicy.Name`: `no`, `always`, `unless-stopped`, or
    /// absent. A one-shot job declares `no`.
    pub restart_policy: Option<String>,
}

impl ServiceHealth {
    /// A service is ready when it runs and its own check does not say
    /// otherwise. A container with no check is ready as soon as it runs;
    /// a declared check that is still `starting` is not ready yet.
    pub fn is_ready(&self) -> bool {
        if self.is_one_shot_complete() {
            return true;
        }
        self.state.eq_ignore_ascii_case("running")
            && !matches!(
                self.health,
                Some(HealthState::Unhealthy | HealthState::Starting)
            )
    }

    /// A one-shot job that ran to completion. It declared `restart: "no"`,
    /// which is exactly the promise that it does not come back, and it exited
    /// successfully; that is the service doing its job, not a failed start
    /// (effigy#371). A `restart: "no"` service that exited non-zero is still
    /// unready, and so is every service the runtime restarts.
    pub fn is_one_shot_complete(&self) -> bool {
        self.has_exited()
            && self.exit_code == Some(0)
            && matches!(self.restart_policy.as_deref(), Some("" | "no") | None)
    }

    /// A container that started and then stopped. A `created` container was
    /// never started, so it is reported as not running, not as an exit.
    pub fn has_exited(&self) -> bool {
        matches!(self.state.to_ascii_lowercase().as_str(), "exited" | "dead")
    }

    /// What is wrong, in words: `web is unhealthy after 12 failed checks`.
    pub fn problem(&self) -> String {
        if self.has_exited() {
            let code = self
                .exit_code
                .map(|code| format!(" with exit code {code}"))
                .unwrap_or_default();
            return format!("{} exited{code}", self.service);
        }
        match self.health {
            Some(HealthState::Unhealthy) => format!(
                "{} is unhealthy after {} failed check{}",
                self.service,
                self.failing_streak,
                if self.failing_streak == 1 { "" } else { "s" }
            ),
            Some(HealthState::Starting) => {
                format!("{} is still starting its health check", self.service)
            }
            _ => format!("{} is not running ({})", self.service, self.state),
        }
    }

    /// The check output that identifies the failure, normalized so two
    /// services failing the same way compare equal.
    pub fn failure_signature(&self) -> Option<String> {
        self.last_output
            .as_deref()
            .map(normalize_signature)
            .filter(|signature| !signature.is_empty())
    }
}

/// The one line of a check's own output the diagnostic keeps: the last line
/// that names a cause, else the last line the check wrote, through the shared
/// redactor and bound, and stripped of the control characters that make a
/// rendered error unreadable.
pub fn bounded_last_output(output: &str) -> Option<String> {
    let line = effigy_ui::human::last_output_line(output)?;
    let cleaned = line
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    Some(cleaned.trim().to_owned())
}

fn normalize_signature(output: &str) -> String {
    bounded_last_output(output)
        .unwrap_or_default()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

/// Parse `docker inspect` / `nerdctl inspect` JSON into per-service readiness.
///
/// Every field the diagnostic needs is required to report it, but a field the
/// runtime omits is not an error: a container with no health check simply has
/// no health state, and a running one has no exit code.
pub fn parse_inspect_health(bytes: &[u8]) -> Result<Vec<ServiceHealth>, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Record {
        #[serde(default)]
        name: String,
        #[serde(default)]
        state: Option<State>,
        #[serde(default)]
        config: Option<Config>,
        #[serde(default)]
        host_config: Option<HostConfig>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct HostConfig {
        #[serde(default)]
        restart_policy: Option<RestartPolicy>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct RestartPolicy {
        #[serde(default)]
        name: Option<String>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct State {
        #[serde(default, alias = "status")]
        status: String,
        #[serde(default)]
        health: Option<Health>,
        #[serde(default)]
        exit_code: Option<i64>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Health {
        #[serde(default)]
        status: String,
        #[serde(default)]
        failing_streak: u64,
        #[serde(default)]
        log: Vec<HealthLog>,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct HealthLog {
        #[serde(default)]
        output: String,
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Config {
        #[serde(default)]
        labels: Option<BTreeMap<String, String>>,
    }
    let records: Vec<Record> = serde_json::from_slice(bytes)
        .map_err(|error| format!("unreadable inspect output: {error}"))?;
    if records.is_empty() {
        return Err("inspect returned no container record".to_owned());
    }
    Ok(records
        .into_iter()
        .map(|record| {
            let container = record.name.trim_start_matches('/').to_owned();
            let state = record.state;
            let health = state.as_ref().and_then(|state| state.health.as_ref());
            let restart_policy = record
                .host_config
                .and_then(|host| host.restart_policy)
                .and_then(|policy| policy.name)
                .map(|name| name.trim().to_owned());
            let service = record
                .config
                .and_then(|config| config.labels)
                .and_then(|labels| labels.get("com.docker.compose.service").cloned())
                .filter(|service| !service.trim().is_empty())
                .unwrap_or_else(|| container.clone());
            ServiceHealth {
                service,
                container,
                state: state
                    .as_ref()
                    .map(|state| state.status.trim().to_owned())
                    .unwrap_or_default(),
                health: health.and_then(|health| HealthState::parse(&health.status)),
                failing_streak: health.map(|health| health.failing_streak).unwrap_or(0),
                last_output: health
                    .and_then(|health| health.log.last())
                    .and_then(|log| bounded_last_output(&log.output)),
                exit_code: state.as_ref().and_then(|state| state.exit_code),
                restart_policy,
            }
        })
        .collect())
}

/// Observe every service of `policy`'s Compose project, including stopped
/// ones, and report each container's own readiness.
///
/// The inventory is the authority for which containers belong to the project;
/// `inspect` is the authority for what each one reports. An unavailable
/// runtime is an error, never "nothing is wrong".
pub fn inspect_project_health(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
) -> Result<Vec<ServiceHealth>, ContainerExecError> {
    inspect_project_health_observed(repo_root, policy, deadline, &mut |_| {})
}

/// One runtime read a project health probe makes, in the order it makes them.
///
/// The probe is two calls, not one: it lists the project's containers and then
/// inspects those names. A caller that shows progress names the stage now in
/// flight, so a stalled operation is never reported as one generic health read
/// (effigy#460, Q-017).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthProbeStage<'a> {
    /// Listing the project's containers, running and stopped.
    ListingContainers,
    /// Inspecting these containers' own health.
    InspectingContainers { containers: &'a [String] },
}

/// The same probe, reporting each runtime read as it starts.
pub fn inspect_project_health_observed(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    deadline: Option<Instant>,
    stage: &mut dyn FnMut(HealthProbeStage<'_>),
) -> Result<Vec<ServiceHealth>, ContainerExecError> {
    stage(HealthProbeStage::ListingContainers);
    let rows = list_compose_containers_for_policy_including_stopped_with_deadline(
        repo_root, policy, deadline,
    )?;
    let names = rows
        .iter()
        .filter(|row| !row.oneoff)
        .map(|row| row.container_name.clone())
        .collect::<Vec<_>>();
    if names.is_empty() {
        return Ok(Vec::new());
    }
    stage(HealthProbeStage::InspectingContainers { containers: &names });
    let output = inspect_containers_for_policy(repo_root, policy, &names, deadline)?;
    let mut health =
        parse_inspect_health(&output.stdout).map_err(|reason| ContainerExecError::Failure {
            command: "runtime inspect".to_owned(),
            code: output.status.code(),
            stdout: String::new(),
            stderr: reason,
        })?;
    // The Compose inventory knows the service a container belongs to even when
    // an inspect record carries no label; prefer the label, fall back here.
    let services = rows
        .into_iter()
        .filter_map(|row| row.service.map(|service| (row.container_name, service)))
        .collect::<BTreeMap<_, _>>();
    for entry in &mut health {
        if entry.service == entry.container {
            if let Some(service) = services.get(&entry.container) {
                entry.service = service.clone();
            }
        }
    }
    health.sort_by(|left, right| left.service.cmp(&right.service));
    Ok(health)
}

/// Every service that is not ready, in a stable order.
pub fn unready(services: &[ServiceHealth]) -> Vec<&ServiceHealth> {
    services
        .iter()
        .filter(|service| !service.is_ready())
        .collect()
}

/// The services whose own health check keeps failing the same way, when every
/// one of them does.
///
/// A stack whose images lack the command their checks run fails identically on
/// every service; waiting out the deadline teaches nothing, so the bring-up
/// can say what is wrong immediately instead.
pub fn shared_failure(services: &[ServiceHealth]) -> Option<ContinuousHealthCheckFailure> {
    let unready = unready(services);
    if unready.len() < 2 {
        return None;
    }
    let mut signature: Option<String> = None;
    let mut names = Vec::new();
    for service in unready {
        let candidate = service.failure_signature()?;
        match &signature {
            None => signature = Some(candidate),
            Some(existing) if *existing == candidate => {}
            Some(_) => return None,
        }
        names.push(service.service.clone());
    }
    Some(ContinuousHealthCheckFailure {
        signature: signature?,
        services: names,
        consecutive_checks: 1,
    })
}

/// Render what is wrong with an environment that did not become ready: the
/// failing services and their evidence, then one `next:` line. `logs` holds the
/// last log line of each exited container. `last_read_error` is set when the
/// final health read itself failed, so the runtime is what a person inspects.
pub fn readiness_diagnostic(
    environment: &str,
    services: &[ServiceHealth],
    timeout_secs: u64,
    logs: &BTreeMap<String, String>,
    last_read_error: Option<&str>,
) -> String {
    let mut lines = vec![format!(
        "environment `{environment}` did not become ready within {timeout_secs}s"
    )];
    let unready = unready(services);
    if unready.is_empty() {
        lines
            .push("every service reported healthy, so readiness could not be confirmed".to_owned());
    }
    for service in &unready {
        let mut line = format!("- {} ({})", service.problem(), service.container);
        if let Some(output) = service.last_output.as_deref() {
            line.push_str(&format!("; its last health check said: `{output}`"));
        }
        if service.has_exited() {
            if let Some(log) = logs.get(&service.service) {
                line.push_str(&format!("; its last log line: `{log}`"));
            }
        }
        lines.push(line);
    }
    if let Some(error) = last_read_error {
        lines.push(format!("the last health read itself failed: {error}"));
    }
    lines.push(format!(
        "next: {}",
        readiness_next_action(&unready, last_read_error.is_some())
    ));
    lines.join("\n")
}

/// The one action that moves a stuck bring-up on. A failed read means the
/// runtime is the thing to inspect; otherwise the first service a person reads
/// is the one to fix, and its output is the way in.
fn readiness_next_action(unready: &[&ServiceHealth], read_failed: bool) -> String {
    if read_failed {
        return "inspect the container runtime with `effigy doctor`, then rerun `effigy container up`".to_owned();
    }
    let Some(first) = unready
        .iter()
        .find(|service| service.has_exited())
        .or(unready.first())
    else {
        return "inspect the stack with `effigy container status`".to_owned();
    };
    let service = &first.service;
    let fix = if first.has_exited() {
        "fix its configuration"
    } else {
        "fix its health check or image"
    };
    format!(
        "read `{service}`'s output with `effigy container logs --service {service}`, {fix}, then rerun `effigy container up`; retrying unchanged fails the same way"
    )
}

/// The diagnostic for a stack whose unready services fail identically: the
/// services and the shared check output, then the one next action.
pub fn repeated_failure_diagnostic(failure: &ContinuousHealthCheckFailure) -> String {
    let service = failure
        .services
        .first()
        .map(String::as_str)
        .unwrap_or_default();
    format!(
        "{} fail the same health check: `{}`\nthe services are running, so the health check or the image is at fault, not the stack\nnext: read `{service}`'s check output with `effigy container logs --service {service}`, fix the health check or the image, then rerun `effigy container up`; retrying unchanged fails the same way",
        failure.services.join(", "),
        failure.signature
    )
}

/// How many consecutive observations the shared failure must repeat before a
/// bring-up stops waiting: one sample can catch a check mid-restart.
pub const SHARED_FAILURE_OBSERVATIONS: u32 = 4;

/// The shortest wait that still lets one check run twice, so a transient
/// failure is not diagnosed as permanent.
pub const SHARED_FAILURE_MIN_ELAPSED: Duration = Duration::from_secs(2);

/// Track the shared failure across observations and report it once it has
/// repeated for [`SHARED_FAILURE_OBSERVATIONS`] observations.
#[derive(Debug, Default)]
pub struct SharedFailureTracker {
    observed: Option<String>,
    consecutive: u32,
}

impl SharedFailureTracker {
    /// Feed one observation. Returns the diagnosis once the same shared
    /// failure has repeated often enough *and* the wait has run long enough
    /// for a check to have run more than once.
    pub fn observe(
        &mut self,
        services: &[ServiceHealth],
        elapsed: Duration,
    ) -> Option<ContinuousHealthCheckFailure> {
        let Some(mut failure) = shared_failure(services) else {
            self.observed = None;
            self.consecutive = 0;
            return None;
        };
        match self.observed.as_deref() {
            Some(signature) if signature == failure.signature => {
                self.consecutive = self.consecutive.saturating_add(1);
            }
            _ => {
                self.observed = Some(failure.signature.clone());
                self.consecutive = 1;
            }
        }
        failure.consecutive_checks = self.consecutive;
        (self.consecutive >= SHARED_FAILURE_OBSERVATIONS && elapsed >= SHARED_FAILURE_MIN_ELAPSED)
            .then_some(failure)
    }
}

#[cfg(test)]
#[path = "readiness/tests.rs"]
mod tests;
