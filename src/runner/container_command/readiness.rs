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
use effigy_tui::waits::{EntryPhase, EntryState, WaitOn};

use crate::runner::error::RunnerError;
use crate::runner::managed_wait::WaitLedger;

use super::steps::TimedStep;

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

/// Wait until every service of `policy`'s environment reports ready.
///
/// The wait is bounded by `deadline`, the one budget the bring-up shares with
/// the runtime reads it made before this wait; without one it starts the
/// environment's own `health` timeout now. One step per service says what is
/// happening: `waiting for web to report healthy (attempt 4, last: HTTP 404)`.
pub(super) fn await_services_ready(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&AtomicBool>,
    deadline: Option<Instant>,
) -> Result<ReadinessEnd, RunnerError> {
    let waits = WaitLedger::environment(&policy.repo_root, &policy.name);
    // A new session's phase begins settled and without counts: a record left by
    // an earlier run is never read while this one is still probing, and the
    // staged host graph in the same scope is untouched (effigy#458).
    begin_environment_phase(&waits);
    let result = await_services_ready_inner(repo_root, policy, stop_requested, deadline, &waits);
    // Every exit is terminal: a success, a nothing-to-await fallback or a stop
    // settles the phase, and a failure or timeout names what ended it, so
    // status never reports a live wait after the phase is over.
    end_environment_phase(&waits, &result);
    result
}

/// The phase's start: settled and count-free, so nothing from an earlier
/// session is read as this one's.
fn begin_environment_phase(waits: &WaitLedger) {
    waits.settle_phase();
}

/// The phase's end, on every exit.
fn end_environment_phase(waits: &WaitLedger, result: &Result<ReadinessEnd, RunnerError>) {
    match result {
        Ok(_) => waits.settle_phase(),
        Err(error) => {
            // The headline and the first piece of evidence, bounded: a person
            // reading `status` needs the failing service, not just the phase.
            let text = error.to_string();
            let mut lines = text.lines();
            let headline = lines.next().unwrap_or_default().trim();
            let evidence = lines
                .map(str::trim)
                .find(|line| line.starts_with("- "))
                .map(|line| line.trim_start_matches("- ").trim())
                .unwrap_or_default();
            let detail = if evidence.is_empty() {
                headline.to_owned()
            } else {
                format!("{headline}; {evidence}")
            };
            waits.publish_phase(&EntryState {
                entry: "environment".to_owned(),
                phase: EntryPhase::Failed,
                waiting_on: None,
                not_started_because: None,
                detail: Some(detail.chars().take(240).collect()),
                elapsed_ms: 0,
            });
        }
    }
}

fn await_services_ready_inner(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    stop_requested: Option<&AtomicBool>,
    deadline: Option<Instant>,
    waits: &WaitLedger,
) -> Result<ReadinessEnd, RunnerError> {
    let started = Instant::now();
    let deadline = deadline.unwrap_or_else(|| observation_deadline(policy));
    let parent = TimedStep::new("waiting for services to report healthy");
    let mut steps = BTreeMap::<String, TimedStep>::new();
    let mut attempts = BTreeMap::<String, u32>::new();
    let mut tracker = SharedFailureTracker::default();
    let mut last_services: Vec<ServiceHealth> = Vec::new();
    let mut last_probe_error: Option<String> = None;
    let mut observed = false;

    loop {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            for step in steps.values() {
                step.finish_clear();
            }
            parent.finish_clear();
            return Ok(ReadinessEnd::Interrupted);
        }
        let probe_deadline = probe_budget_deadline(Instant::now(), deadline);
        match inspect_project_health(repo_root, policy, Some(probe_deadline)) {
            Ok(services) if services.is_empty() => {
                for step in steps.values() {
                    step.finish_clear();
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
                let ready = report_services(&services, &mut steps, &mut attempts);
                publish_environment_wait(waits, &services, &ready, started.elapsed());
                if ready.is_empty() {
                    for step in steps.values() {
                        step.finish_clear();
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
                        if let Some(step) = steps.get(&service.service) {
                            step.fail(service.problem());
                        }
                    }
                    // The step ends on the diagnosis's first line; the error
                    // carries the evidence and its `next:` line.
                    parent.fail(diagnosis.lines().next().unwrap_or_default());
                    return Err(RunnerError::task_invocation(diagnosis));
                }
            }
            Err(error) => {
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
        for step in steps.values() {
            step.finish_clear();
        }
        parent.finish_clear();
        return Ok(ReadinessEnd::NothingToAwait);
    }
    readiness_failure(policy, &last_services, last_probe_error, &steps)
}

/// Publishes the containers that are healthy now, as the wait every entry
/// that waits for this environment renders.
fn publish_environment_wait(
    waits: &WaitLedger,
    services: &[ServiceHealth],
    unready: &[String],
    elapsed: Duration,
) {
    let total = u32::try_from(services.len()).unwrap_or(u32::MAX);
    let healthy = u32::try_from(services.len().saturating_sub(unready.len())).unwrap_or(u32::MAX);
    waits.publish_phase(&EntryState {
        entry: "environment".to_owned(),
        phase: EntryPhase::Waiting,
        waiting_on: Some(WaitOn::Environment {
            healthy: Some(healthy),
            total: Some(total),
        }),
        not_started_because: None,
        detail: None,
        elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
    });
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
    steps: &BTreeMap<String, TimedStep>,
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
        if let Some(step) = steps.get(&service.service) {
            step.fail(service.problem());
        }
    }
    Err(RunnerError::task_invocation(diagnostic))
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

/// Update one step per service and return the ones that are not ready.
fn report_services(
    services: &[ServiceHealth],
    steps: &mut BTreeMap<String, TimedStep>,
    attempts: &mut BTreeMap<String, u32>,
) -> Vec<String> {
    let mut not_ready = Vec::new();
    for service in services {
        if service.is_ready() {
            if let Some(step) = steps.get(&service.service) {
                step.succeed(format!("{} is healthy", service.service));
            }
            continue;
        }
        let attempt = attempts.entry(service.service.clone()).or_insert(0);
        *attempt += 1;
        let label = match service.last_output.as_deref() {
            Some(output) => format!(
                "waiting for {} to report healthy (attempt {attempt}, last: {output})",
                service.service
            ),
            None => format!(
                "waiting for {} to report healthy (attempt {attempt}, {})",
                service.service, service.state
            ),
        };
        match steps.get(&service.service) {
            Some(step) => step.set_label(label),
            None => {
                steps.insert(service.service.clone(), TimedStep::new(label));
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

    #[test]
    fn a_waiting_step_names_the_service_the_attempt_and_the_last_check_output() {
        let services = vec![
            ServiceHealth {
                service: "web".to_owned(),
                container: "demo-web-1".to_owned(),
                state: "running".to_owned(),
                health: Some(HealthState::Unhealthy),
                failing_streak: 4,
                last_output: Some("GET / HTTP/1.1 404".to_owned()),
                exit_code: None,
                restart_policy: None,
            },
            ServiceHealth {
                service: "db".to_owned(),
                container: "demo-db-1".to_owned(),
                state: "running".to_owned(),
                health: Some(HealthState::Healthy),
                failing_streak: 0,
                last_output: None,
                exit_code: None,
                restart_policy: None,
            },
        ];
        let mut steps = BTreeMap::new();
        let mut attempts = BTreeMap::new();
        let not_ready = report_services(&services, &mut steps, &mut attempts);
        assert_eq!(not_ready, vec!["web"]);
        assert_eq!(attempts.get("web"), Some(&1));
        let step = steps.get("web").expect("web is waiting");
        // The drawing is silent under test; the label is what a terminal shows.
        assert_eq!(
            step.label(),
            "waiting for web to report healthy (attempt 1, last: GET / HTTP/1.1 404)"
        );
        // The healthy service has no waiting step.
        assert!(!steps.contains_key("db"));
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
    fn phase_counts(healthy: u32, total: u32) -> EntryState {
        EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(healthy),
                total: Some(total),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        }
    }

    /// The phase is terminal on every exit, and a new session never reads the
    /// previous one's counts (effigy#458).
    #[test]
    fn the_phase_settles_on_every_exit_and_a_new_session_starts_clean() {
        let dir = tempfile::tempdir().expect("fixture directory");
        let ledger = WaitLedger::environment(dir.path(), "app");
        ledger.clear();

        // A previous run left its live counts behind.
        ledger.publish_phase(&phase_counts(1, 4));
        assert_eq!(ledger.read_progress(), Some((1, 4)));

        // The new session's boundary resets the phase without touching a host
        // snapshot staged in the same scope.
        let host = EntryState {
            entry: "front".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(1),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        };
        ledger.publish(&host);
        begin_environment_phase(&ledger);
        assert_eq!(
            ledger.read_progress(),
            None,
            "the previous session's counts are never read"
        );
        assert_eq!(
            ledger.read("front").and_then(|state| state.waiting_on),
            Some(WaitOn::Environment {
                healthy: Some(1),
                total: Some(4)
            }),
            "the staged host snapshot is untouched"
        );

        // A success settles the phase.
        ledger.publish_phase(&phase_counts(4, 4));
        end_environment_phase(&ledger, &Ok(ReadinessEnd::Ready));
        assert_eq!(ledger.read_progress(), None);
        assert!(!ledger
            .read_phase()
            .is_some_and(|phase| phase.holds_a_wait()));

        // A failure publishes the phase's own attributed terminal record.
        ledger.publish_phase(&phase_counts(2, 4));
        let failure = RunnerError::task_invocation(
            "environment `app` did not become ready within 60s\n- web is unhealthy after 3 failed checks",
        );
        end_environment_phase(&ledger, &Err(failure));
        let phase = ledger.read_phase().expect("the failed phase record");
        assert_eq!(phase.phase, EntryPhase::Failed);
        assert!(
            phase
                .detail
                .as_deref()
                .unwrap_or_default()
                .contains("web is unhealthy"),
            "{phase:?}"
        );
        assert_eq!(ledger.read_progress(), None, "no live counts survive");

        // A stop or a nothing-to-await fallback settles too.
        ledger.publish_phase(&phase_counts(3, 4));
        end_environment_phase(&ledger, &Ok(ReadinessEnd::Interrupted));
        assert_eq!(ledger.read_progress(), None);
        ledger.publish_phase(&phase_counts(3, 4));
        end_environment_phase(&ledger, &Ok(ReadinessEnd::NothingToAwait));
        assert_eq!(ledger.read_progress(), None);
    }
}
