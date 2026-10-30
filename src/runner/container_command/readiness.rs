//! Waiting for an environment's services to report ready, with the evidence.
//!
//! A bring-up is not finished when Compose returns: it is finished when every
//! service says it is ready. This wait names each service while it waits, and
//! when it cannot finish it says which service is wrong, what its own health
//! check said, and what to run next (effigy#313). A stack whose unready
//! services all fail the same way is diagnosed instead of waited out.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use effigy_containers::exec::container_last_log_line_for_policy;
use effigy_containers::health::readiness::{
    inspect_project_health, readiness_diagnostic, repeated_failure_diagnostic, unready,
    ServiceHealth, SharedFailureTracker,
};
use effigy_containers::EffectiveContainerPolicy;

use crate::runner::error::RunnerError;

use super::steps::{ProgressSource, TimedStep};

/// How often the wait reads the services' own health while a stack is still
/// starting. Every probe is two runtime calls, so the wait backs off to
/// [`PROBE_INTERVAL_SETTLED`] once the stack has had a moment to come up.
const PROBE_INTERVAL_STARTING: Duration = Duration::from_millis(400);
/// The probe interval after [`PROBE_STARTING_WINDOW`].
const PROBE_INTERVAL_SETTLED: Duration = Duration::from_millis(1_500);
/// How long the wait keeps the fast probe.
const PROBE_STARTING_WINDOW: Duration = Duration::from_secs(4);
/// How long one probe may take. A probe is a read, so it never consumes the
/// wait's whole budget, and an unresponsive runtime is reported rather than
/// waited on.
const PROBE_BUDGET: Duration = Duration::from_secs(15);
/// The smallest budget a probe gets. The wait's last probe is what produces
/// the readiness diagnostic, so it must still have time to answer when it
/// starts at the deadline; otherwise the diagnostic ends with a "timed out
/// after 0s" read that never happened.
const PROBE_BUDGET_MINIMUM: Duration = Duration::from_secs(5);
/// How many trailing log lines one exited service is read for.
// Enough lines that a service's own cause line is still in the tail when its
// last lines are shutdown notes (MariaDB writes `[ERROR]` before "shutdown
// complete").
const EXITED_LOG_TAIL: u32 = 20;

/// How the wait ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadinessEnd {
    /// Every service reported ready.
    Ready,
    /// No service reported ready either: the environment declares no Compose
    /// services this runtime can see, so the caller's declared check decides.
    NothingToAwait,
    /// A stop was requested.
    Interrupted,
}

/// One service's own step while it is not yet healthy.
struct ServiceStep {
    progress: ProgressSource,
    step: TimedStep,
}

/// Wait until every service of `policy`'s environment reports ready.
///
/// The wait is bounded by `deadline`, the one budget the bring-up shares with
/// the runtime reads it made before this wait; without one it starts the
/// environment's own `health` timeout now. The parent step counts the healthy
/// services and names the one still failing; each waiting service has its own
/// step with its attempt and last check output (effigy#460, Q-017).
pub(super) fn await_services_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&AtomicBool>,
    deadline: Option<Instant>,
) -> Result<ReadinessEnd, RunnerError> {
    let started = Instant::now();
    let deadline = deadline.unwrap_or_else(|| observation_deadline(policy));
    let progress = ProgressSource::waiting_for("the environment's services to report healthy");
    progress.update(
        format!(
            "reading the health of every service in project `{}`",
            policy.project_name
        ),
        None,
        None,
    );
    let parent = TimedStep::live(progress.clone());
    let mut steps = BTreeMap::<String, ServiceStep>::new();
    let mut attempts = BTreeMap::<String, u32>::new();
    let mut tracker = SharedFailureTracker::default();
    let mut last_services: Vec<ServiceHealth> = Vec::new();
    let mut last_probe_error: Option<String> = None;
    let mut observed = false;

    loop {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            for waiting in steps.values() {
                waiting.step.finish_clear();
            }
            parent.finish_clear();
            return Ok(ReadinessEnd::Interrupted);
        }
        let probe_deadline = probe_budget_deadline(Instant::now(), deadline);
        match inspect_project_health(repo_root, policy, Some(probe_deadline)) {
            Ok(services) if services.is_empty() => {
                for waiting in steps.values() {
                    waiting.step.finish_clear();
                }
                parent.finish_clear();
                return Ok(if observed && last_probe_error.is_some() {
                    readiness_failure(policy, &last_services, last_probe_error, &steps)?
                } else {
                    ReadinessEnd::NothingToAwait
                });
            }
            Ok(services) => {
                observed = true;
                last_probe_error = None;
                last_services = services.clone();
                observe_parent(&progress, &services);
                let ready = report_services(&services, &mut steps, &mut attempts);
                if ready.is_empty() {
                    for waiting in steps.values() {
                        waiting.step.finish_clear();
                    }
                    parent.succeed(format!(
                        "{} service{} ready",
                        services.len(),
                        if services.len() == 1 { "" } else { "s" }
                    ));
                    return Ok(ReadinessEnd::Ready);
                }
                // A stack whose unready services all fail the same way will
                // not fix itself: stop the wait with that diagnosis.
                if let Some(failure) = tracker.observe(&services, started.elapsed()) {
                    let diagnosis = repeated_failure_diagnostic(&failure);
                    for service in unready(&services) {
                        if let Some(waiting) = steps.get(&service.service) {
                            waiting.step.fail(service.problem());
                        }
                    }
                    // The step ends on the diagnosis's first line; the error
                    // carries the evidence and its `next:` line.
                    parent.fail(diagnosis.lines().next().unwrap_or_default());
                    return Err(readiness_wait_error(
                        policy,
                        diagnosis,
                        waiting_on_entries(&services, None, &policy.project_name),
                    ));
                }
            }
            Err(error) => {
                observe_probe_failure(&progress, &policy.project_name, &error.to_string());
                last_probe_error = Some(error.to_string());
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        let interval = if started.elapsed() < PROBE_STARTING_WINDOW {
            PROBE_INTERVAL_STARTING
        } else {
            PROBE_INTERVAL_SETTLED
        };
        std::thread::sleep(interval.min(deadline.saturating_duration_since(Instant::now())));
    }

    if !observed {
        // No service's health could be read at all, so the environment's own
        // declared check is the only evidence there is. This is exactly the
        // behaviour before per-service readiness, never a new refusal.
        for waiting in steps.values() {
            waiting.step.finish_clear();
        }
        parent.finish_clear();
        return Ok(ReadinessEnd::NothingToAwait);
    }
    readiness_failure(policy, &last_services, last_probe_error, &steps)
}

/// The parent step after a probe that read every service: the services it is
/// waiting on by name, how many are healthy, and the first one still failing
/// in its own words.
fn observe_parent(progress: &ProgressSource, services: &[ServiceHealth]) {
    let ready = services.iter().filter(|service| service.is_ready()).count();
    let waiting = unready(services);
    let names = waiting
        .iter()
        .map(|service| service.service.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let sub_step = if names.is_empty() {
        "waiting for services to report healthy".to_owned()
    } else {
        format!("waiting for {names} to report healthy")
    };
    let latest = waiting.first().map(|service| service.problem());
    progress.update(
        sub_step,
        Some((ready as u64, services.len() as u64)),
        latest,
    );
}

/// The waits a readiness wait ended on, machine-readable: one entry per
/// service still unready, and one for a health read the runtime did not answer.
fn waiting_on_entries(
    services: &[ServiceHealth],
    probe_error: Option<&str>,
    project: &str,
) -> Vec<serde_json::Value> {
    let mut waits: Vec<serde_json::Value> = unready(services)
        .into_iter()
        .map(|service| {
            serde_json::json!({
                "kind": "service",
                "service": service.service,
                "container": service.container,
                "state": service.state,
                "problem": service.problem(),
            })
        })
        .collect();
    if let Some(error) = probe_error {
        waits.push(serde_json::json!({
            "kind": "probe",
            "project": project,
            "probe": "health read",
            "error": first_line(error),
        }));
    }
    waits
}

/// The readiness failure: the human diagnostic, and the same waits as the
/// machine-readable `waiting_on` list.
fn readiness_wait_error(
    policy: &EffectiveContainerPolicy,
    diagnostic: String,
    waiting_on: Vec<serde_json::Value>,
) -> RunnerError {
    RunnerError::ContainerReadinessWait {
        environment: policy.name.clone(),
        diagnostic,
        details_json: serde_json::json!({
            "environment": policy.name,
            "readiness": "unverified",
            "waiting_on": waiting_on,
        })
        .to_string(),
    }
}

/// The parent step after a probe the runtime could not answer: it names the
/// exact health read it is waiting on, with the error that read returned.
fn observe_probe_failure(progress: &ProgressSource, project: &str, error: &str) {
    progress.update(
        format!("waiting for the runtime to answer the health read of project `{project}`"),
        None,
        Some(first_line(error)),
    );
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

/// When one probe must give up: at most [`PROBE_BUDGET`] away, but never less
/// than [`PROBE_BUDGET_MINIMUM`] even at the wait's deadline. A probe that
/// starts at the deadline is the last diagnostic read, not a reason to report
/// the runtime as silent.
fn probe_budget_deadline(now: Instant, deadline: Instant) -> Instant {
    deadline
        .min(now + PROBE_BUDGET)
        .max(now + PROBE_BUDGET_MINIMUM)
}

/// The one deadline a bring-up's runtime observations share, measured from the
/// moment Compose returned: the environment's own health budget bounds its
/// inventory reads and its readiness wait together, so a runtime that stops
/// answering spends that budget once and then answers with the refusal
/// (effigy#411).
pub(super) fn observation_deadline(policy: &EffectiveContainerPolicy) -> Instant {
    Instant::now() + Duration::from_secs(policy.health_timeout_secs.max(1))
}

/// Build the failure for a wait that ran out of time.
fn readiness_failure(
    policy: &EffectiveContainerPolicy,
    services: &[ServiceHealth],
    probe_error: Option<String>,
    steps: &BTreeMap<String, ServiceStep>,
) -> Result<ReadinessEnd, RunnerError> {
    let logs = exited_logs(&policy.repo_root, policy, services);
    let diagnostic = readiness_diagnostic(
        &policy.name,
        services,
        policy.health_timeout_secs,
        &logs,
        probe_error.as_deref(),
    );
    for service in unready(services) {
        if let Some(waiting) = steps.get(&service.service) {
            waiting.step.fail(service.problem());
        }
    }
    let waiting_on = waiting_on_entries(services, probe_error.as_deref(), &policy.project_name);
    Err(readiness_wait_error(policy, diagnostic, waiting_on))
}

/// The last log line of every service that stopped instead of starting.
///
/// Only exited services are read: a healthy stack pays nothing for a
/// diagnostic it does not need, and an unhealthy one gets the evidence its
/// health check could not give.
fn exited_logs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    services: &[ServiceHealth],
) -> BTreeMap<String, String> {
    let mut logs = BTreeMap::new();
    for service in services.iter().filter(|service| service.has_exited()) {
        let Ok(line) = container_last_log_line_for_policy(
            repo_root,
            policy,
            &service.container,
            EXITED_LOG_TAIL,
        ) else {
            continue;
        };
        if !line.is_empty() {
            logs.insert(service.service.clone(), line);
        }
    }
    logs
}

/// Update one step per service and return the ones that are not ready. A
/// waiting service's frame names its attempt and the last thing its health
/// check or container state reported.
fn report_services(
    services: &[ServiceHealth],
    steps: &mut BTreeMap<String, ServiceStep>,
    attempts: &mut BTreeMap<String, u32>,
) -> Vec<String> {
    let mut not_ready = Vec::new();
    for service in services {
        if service.is_ready() {
            if let Some(waiting) = steps.get(&service.service) {
                waiting
                    .step
                    .succeed(format!("{} is healthy", service.service));
            }
            continue;
        }
        let attempt = attempts.entry(service.service.clone()).or_insert(0);
        *attempt += 1;
        let sub_step = format!(
            "waiting for {} to report healthy (attempt {attempt})",
            service.service
        );
        let last = service
            .last_output
            .clone()
            .unwrap_or_else(|| format!("container {}", service.state));
        match steps.get(&service.service) {
            Some(waiting) => waiting.progress.update(sub_step, None, Some(last)),
            None => {
                let progress =
                    ProgressSource::waiting_for(format!("{} to report healthy", service.service));
                progress.update(sub_step, None, Some(last));
                let step = TimedStep::live(progress.clone());
                steps.insert(service.service.clone(), ServiceStep { progress, step });
            }
        }
        not_ready.push(service.service.clone());
    }
    not_ready
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_containers::health::readiness::parse_inspect_health;
    use effigy_containers::health::readiness::HealthState;

    fn health(
        service: &str,
        state: &str,
        health: Option<HealthState>,
        streak: u32,
        output: Option<&str>,
    ) -> ServiceHealth {
        ServiceHealth {
            service: service.to_owned(),
            container: format!("demo-{service}-1"),
            state: state.to_owned(),
            health,
            failing_streak: u64::from(streak),
            last_output: output.map(str::to_owned),
            exit_code: None,
            restart_policy: None,
        }
    }

    #[test]
    fn a_waiting_service_names_its_attempt_and_last_check_output() {
        let services = vec![
            health(
                "web",
                "running",
                Some(HealthState::Unhealthy),
                4,
                Some("GET / HTTP/1.1 404"),
            ),
            health("db", "running", Some(HealthState::Healthy), 0, None),
        ];
        let mut steps = BTreeMap::new();
        let mut attempts = BTreeMap::new();
        let not_ready = report_services(&services, &mut steps, &mut attempts);
        assert_eq!(not_ready, vec!["web"]);
        assert_eq!(attempts.get("web"), Some(&1));
        let waiting = steps.get("web").expect("web is waiting");
        // Fake clock: the frame names the attempt, the last output and the time.
        assert_eq!(
            waiting.progress.frame_at(Duration::from_secs(12)),
            "waiting for web to report healthy (attempt 1); last: GET / HTTP/1.1 404; 12s elapsed"
        );
        // The healthy service has no waiting step.
        assert!(!steps.contains_key("db"));
    }

    #[test]
    fn the_parent_counts_healthy_services_and_names_the_failing_one() {
        let services = vec![
            health(
                "web",
                "running",
                Some(HealthState::Unhealthy),
                4,
                Some("GET / HTTP/1.1 404"),
            ),
            health("db", "running", Some(HealthState::Healthy), 0, None),
            health("cache", "running", Some(HealthState::Healthy), 0, None),
        ];
        let progress = ProgressSource::waiting_for("the environment's services to report healthy");
        observe_parent(&progress, &services);
        assert_eq!(
            progress.frame_at(Duration::from_secs(12)),
            "waiting for web to report healthy (2/3); last: web is unhealthy after 4 failed checks; 12s elapsed"
        );
    }

    #[test]
    fn the_wait_is_machine_readable_per_service_and_per_probe() {
        let services = vec![
            health(
                "web",
                "running",
                Some(HealthState::Unhealthy),
                4,
                Some("GET / HTTP/1.1 404"),
            ),
            health("db", "running", Some(HealthState::Healthy), 0, None),
        ];
        let waits = waiting_on_entries(&services, Some("docker: daemon not responding"), "demo");
        assert_eq!(waits.len(), 2, "{waits:?}");
        assert_eq!(waits[0]["kind"], "service");
        assert_eq!(waits[0]["service"], "web");
        assert_eq!(
            waits[0]["problem"],
            "web is unhealthy after 4 failed checks"
        );
        assert_eq!(waits[1]["kind"], "probe");
        assert_eq!(waits[1]["project"], "demo");
        assert_eq!(waits[1]["error"], "docker: daemon not responding");
    }

    #[test]
    fn a_silent_runtime_names_the_exact_health_read_it_waits_on() {
        let progress = ProgressSource::waiting_for("the environment's services to report healthy");
        observe_probe_failure(
            &progress,
            "demo",
            "docker: daemon not responding\nsecond line",
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(30)),
            "waiting for the runtime to answer the health read of project `demo`; last: docker: daemon not responding; 30s elapsed"
        );
    }

    #[test]
    fn the_last_probe_keeps_a_budget_after_the_wait_deadline() {
        let now = Instant::now();
        let deadline = now + Duration::from_secs(60);
        // Before the deadline, a probe is bounded by its own budget.
        assert_eq!(probe_budget_deadline(now, deadline), now + PROBE_BUDGET);
        // A probe that starts at the deadline still gets the minimum, so the
        // diagnostic never says a read timed out after 0s.
        let at_deadline = now + Duration::from_secs(60);
        assert_eq!(
            probe_budget_deadline(at_deadline, at_deadline),
            at_deadline + PROBE_BUDGET_MINIMUM
        );
        // A probe just before the deadline also gets the minimum, not the
        // sliver of time left.
        let nearly = deadline - Duration::from_millis(1);
        assert_eq!(
            probe_budget_deadline(nearly, deadline),
            nearly + PROBE_BUDGET_MINIMUM
        );
    }

    #[test]
    fn an_unready_bring_up_names_the_service_its_evidence_and_one_next_action() {
        let services = parse_inspect_health(
            br#"[{"Name":"/demo-web-1","State":{"Status":"running","Health":{"Status":"unhealthy","FailingStreak":4,"Log":[{"Output":"GET / HTTP/1.1 404"}]}},"Config":{"Labels":{"com.docker.compose.service":"web"}}}]"#,
        )
        .expect("inspect parses");
        let human = RunnerError::task_invocation(readiness_diagnostic(
            "demo",
            &services,
            60,
            &BTreeMap::new(),
            None,
        ))
        .human_error();

        assert_eq!(
            human.what_failed(),
            "environment `demo` did not become ready within 60s"
        );
        assert!(
            human.evidence_lines().iter().any(|line| line
                .contains("web is unhealthy after 4 failed checks")
                && line.contains("GET / HTTP/1.1 404")),
            "{:?}",
            human.evidence_lines()
        );
        assert!(human
            .next_action_text()
            .is_some_and(|action| action.contains("effigy container logs --service web")));
        assert_eq!(
            human.to_json()["next_action"],
            human.next_action_text().expect("next action")
        );
    }
}
