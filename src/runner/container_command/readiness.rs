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
    inspect_project_health_observed, readiness_diagnostic, repeated_failure_diagnostic, unready,
    HealthProbeStage, ServiceHealth, SharedFailureTracker,
};
use effigy_containers::EffectiveContainerPolicy;
use effigy_tui::waits::{EntryPhase, EntryState, WaitOn};

use crate::runner::error::RunnerError;
use crate::runner::managed_wait::WaitLedger;

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

/// The wait's end and the last complete per-service observation it made.
///
/// `observed` is what the runtime last reported, empty when no probe returned
/// a service list. A caller that needs a service's container identity reads it
/// from here instead of asking the runtime a second time (effigy#460).
pub(super) struct ReadinessReport {
    pub(super) end: ReadinessEnd,
    pub(super) observed: Vec<ServiceHealth>,
}

/// One service's own step while it is not yet healthy.
struct ServiceStep {
    progress: ProgressSource,
    step: TimedStep,
}

/// The runtime read a probe last started, kept through its failure so the retry
/// frame, the human diagnostic and `waiting_on` all name that exact read and
/// its subjects (effigy#460, Q-017).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbeRead {
    /// Which probe attempt this read belonged to.
    probe: u64,
    stage: ProbeReadStage,
}

/// The two reads one project health probe makes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProbeReadStage {
    /// The project's container list.
    Containers,
    /// The inspect of these container names.
    Inspection(Vec<String>),
}

impl ProbeRead {
    /// The project's container list, the first read of a probe.
    fn containers(probe: u64) -> Self {
        Self {
            probe,
            stage: ProbeReadStage::Containers,
        }
    }

    /// The read as a stable machine token.
    fn token(&self) -> &'static str {
        match self.stage {
            ProbeReadStage::Containers => "container_list",
            ProbeReadStage::Inspection(_) => "containers_inspect",
        }
    }

    /// The containers this read covered, when it was an inspect.
    fn inspected(&self) -> Option<&[String]> {
        match &self.stage {
            ProbeReadStage::Containers => None,
            ProbeReadStage::Inspection(containers) => Some(containers),
        }
    }

    /// The exact read in the operator's words, for `project`.
    fn description(&self, project: &str) -> String {
        match &self.stage {
            ProbeReadStage::Containers => format!(
                "the container list of project `{project}` (probe {})",
                self.probe
            ),
            ProbeReadStage::Inspection(containers) => format!(
                "the inspect of {} container{} of project `{project}`: {} (probe {})",
                containers.len(),
                if containers.len() == 1 { "" } else { "s" },
                bounded_names(containers),
                self.probe
            ),
        }
    }
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
) -> Result<ReadinessReport, RunnerError> {
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
fn end_environment_phase(waits: &WaitLedger, result: &Result<ReadinessReport, RunnerError>) {
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
) -> Result<ReadinessReport, RunnerError> {
    let started = Instant::now();
    let deadline = deadline.unwrap_or_else(|| observation_deadline(policy));
    let progress = ProgressSource::waiting_for("the environment's services to report healthy");
    let parent = TimedStep::live(progress.clone());
    let mut steps = BTreeMap::<String, ServiceStep>::new();
    let mut attempts = BTreeMap::<String, u32>::new();
    let mut tracker = SharedFailureTracker::default();
    let mut last_services: Vec<ServiceHealth> = Vec::new();
    let mut last_probe_error: Option<String> = None;
    let mut last_failed_read: Option<ProbeRead> = None;
    let mut observed = false;
    let mut probes = 0_u64;
    let mut known_counts: Option<(u64, u64)> = None;

    loop {
        if stop_requested.is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed)) {
            for waiting in steps.values() {
                waiting.step.finish_clear();
            }
            parent.finish_clear();
            return Ok(ReadinessReport {
                end: ReadinessEnd::Interrupted,
                observed: last_services,
            });
        }
        let probe_deadline = probe_budget_deadline(Instant::now(), deadline);
        probes += 1;
        observe_probe_started(
            &progress,
            &policy.project_name,
            probes,
            probe_deadline.saturating_duration_since(Instant::now()),
            known_counts,
        );
        // The probe is two runtime reads; the frame follows the one in flight,
        // naming the containers the inspect will read, and a failed read keeps
        // that identity for the retry frame and the refusal (effigy#460,
        // Q-017). The closure's borrow ends before the result is reported.
        let budget = probe_deadline.saturating_duration_since(Instant::now());
        let mut in_flight_read: Option<ProbeRead> = None;
        let probe_result = {
            let mut probe_stage = |stage: HealthProbeStage<'_>| {
                in_flight_read = Some(match stage {
                    HealthProbeStage::ListingContainers => ProbeRead::containers(probes),
                    HealthProbeStage::InspectingContainers { containers } => ProbeRead {
                        probe: probes,
                        stage: ProbeReadStage::Inspection(containers.to_vec()),
                    },
                });
                observe_probe_stage(
                    &progress,
                    &policy.project_name,
                    probes,
                    budget,
                    known_counts,
                    stage,
                );
            };
            inspect_project_health_observed(
                repo_root,
                policy,
                Some(probe_deadline),
                &mut probe_stage,
            )
        };
        match probe_result {
            Ok(services) if services.is_empty() => {
                for waiting in steps.values() {
                    waiting.step.finish_clear();
                }
                parent.finish_clear();
                return Ok(if observed && last_probe_error.is_some() {
                    readiness_failure(
                        policy,
                        &last_services,
                        last_probe_error,
                        last_failed_read.as_ref(),
                        &steps,
                        &parent,
                        &progress,
                    )?
                } else {
                    ReadinessReport {
                        end: ReadinessEnd::NothingToAwait,
                        observed: last_services,
                    }
                });
            }
            Ok(services) => {
                observed = true;
                last_probe_error = None;
                last_failed_read = None;
                last_services = services.clone();
                observe_parent(&progress, &services);
                known_counts = Some((
                    services.iter().filter(|service| service.is_ready()).count() as u64,
                    services.len() as u64,
                ));
                let ready = report_services(&services, &mut steps, &mut attempts);
                publish_environment_wait(waits, &services, &ready, started.elapsed());
                if ready.is_empty() {
                    for waiting in steps.values() {
                        waiting.step.finish_clear();
                    }
                    parent.succeed(format!(
                        "{} service{} ready",
                        services.len(),
                        if services.len() == 1 { "" } else { "s" }
                    ));
                    return Ok(ReadinessReport {
                        end: ReadinessEnd::Ready,
                        observed: services.clone(),
                    });
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
                        waiting_on_entries(&services, None, None, &policy.project_name),
                    ));
                }
            }
            Err(error) => {
                // A backend-selection failure can read the same for either
                // read, so the stage is reported from the read that started,
                // not inferred from the error text.
                let read = in_flight_read
                    .clone()
                    .unwrap_or_else(|| ProbeRead::containers(probes));
                observe_probe_failure(&progress, &policy.project_name, &read, &error.to_string());
                last_probe_error = Some(error.to_string());
                last_failed_read = Some(read);
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
        return Ok(ReadinessReport {
            end: ReadinessEnd::NothingToAwait,
            observed: last_services,
        });
    }
    readiness_failure(
        policy,
        &last_services,
        last_probe_error,
        last_failed_read.as_ref(),
        &steps,
        &parent,
        &progress,
    )
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

/// The parent step while one of the probe's two runtime reads is in flight: the
/// operation, its subjects, the probe number and its budget. The frame names
/// the read now running, not the last result, so a stalled `ps` and a stalled
/// `inspect` are told apart (effigy#460, Q-017).
fn observe_probe_stage(
    progress: &ProgressSource,
    project: &str,
    probe: u64,
    budget: Duration,
    known: Option<(u64, u64)>,
    stage: HealthProbeStage<'_>,
) {
    let sub_step = match stage {
        HealthProbeStage::ListingContainers => format!(
            "listing the containers of project `{project}` from the runtime (probe {probe}, up to {}s)",
            budget.as_secs()
        ),
        HealthProbeStage::InspectingContainers { containers } => format!(
            "inspecting {} container{} of project `{project}`: {} (probe {probe}, up to {}s)",
            containers.len(),
            if containers.len() == 1 { "" } else { "s" },
            bounded_names(containers),
            budget.as_secs()
        ),
    };
    progress.update(sub_step, known, None);
}

/// The containers one inspect read covers, bounded so one frame stays short.
fn bounded_names(containers: &[String]) -> String {
    const LIMIT: usize = 3;
    let shown = containers
        .iter()
        .take(LIMIT)
        .map(|container| format!("`{container}`"))
        .collect::<Vec<_>>()
        .join(", ");
    if containers.len() <= LIMIT {
        return shown;
    }
    format!("{shown} and {} more", containers.len() - LIMIT)
}

/// The parent step while one runtime health read is in flight: which probe it
/// is, how long it may take, and the counts and last event already known. The
/// frame names the probe, not the last result, so a slow read is never shown
/// as an idle wait (effigy#460, Q-017).
fn observe_probe_started(
    progress: &ProgressSource,
    project: &str,
    probe: u64,
    budget: Duration,
    known: Option<(u64, u64)>,
) {
    progress.update(
        format!(
            "reading the health of project `{project}` from the runtime (probe {probe}, up to {}s)",
            budget.as_secs()
        ),
        known,
        None,
    );
}

/// The waits a readiness wait ended on, machine-readable: one entry per
/// service still unready, and one for a health read the runtime did not answer.
fn waiting_on_entries(
    services: &[ServiceHealth],
    failed_read: Option<&ProbeRead>,
    error: Option<&str>,
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
    if let (Some(read), Some(error)) = (failed_read, error) {
        let mut entry = serde_json::json!({
            "kind": "probe",
            "project": project,
            "probe": read.token(),
            "probe_number": read.probe,
            "error": first_line(error),
        });
        if let Some(containers) = read.inspected() {
            entry["containers"] = serde_json::json!(containers);
        }
        waits.push(entry);
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
    RunnerError::container_wait_refused(
        &policy.name,
        diagnostic,
        serde_json::json!({
            "failure_kind": "container_readiness",
            "readiness": "unverified",
            "waiting_on": waiting_on,
        }),
    )
}

/// The parent step after a probe the runtime could not answer: it names the
/// exact read and subjects it was in, with the error that read returned.
fn observe_probe_failure(progress: &ProgressSource, project: &str, read: &ProbeRead, error: &str) {
    progress.update(
        format!(
            "waiting for the runtime to answer {}",
            read.description(project)
        ),
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

/// Build the failure for a wait that ran out of time. It always ends the wait
/// with an error, so its report type only satisfies the caller's signature.
fn readiness_failure(
    policy: &EffectiveContainerPolicy,
    services: &[ServiceHealth],
    probe_error: Option<String>,
    failed_read: Option<&ProbeRead>,
    steps: &BTreeMap<String, ServiceStep>,
    parent: &TimedStep,
    parent_progress: &ProgressSource,
) -> Result<ReadinessReport, RunnerError> {
    let logs = exited_logs(&policy.repo_root, policy, services, parent_progress);
    // The diagnostic says which read failed and what it covered; the JSON keeps
    // the same read as a token, its probe number and its container names.
    let read_error = probe_error
        .as_deref()
        .map(|error| probe_read_error(&policy.project_name, failed_read, error));
    let diagnostic = readiness_diagnostic(
        &policy.name,
        services,
        policy.health_timeout_secs,
        &logs,
        read_error.as_deref(),
    );
    for service in unready(services) {
        if let Some(waiting) = steps.get(&service.service) {
            waiting.step.fail(service.problem());
        }
    }
    let waiting_on = waiting_on_entries(
        services,
        failed_read,
        probe_error.as_deref(),
        &policy.project_name,
    );
    // The readiness wait's own step ends on the diagnosis's first line; the
    // error carries the evidence and its `next:` line. A step an earlier
    // branch already cleared ignores this.
    parent.fail(diagnostic.lines().next().unwrap_or_default());
    Err(readiness_wait_error(policy, diagnostic, waiting_on))
}

/// The diagnostic's account of a read that failed: the exact read, the
/// subjects it covered and the error it returned (effigy#460, Q-017).
fn probe_read_error(project: &str, failed_read: Option<&ProbeRead>, error: &str) -> String {
    match failed_read {
        Some(read) => format!("{}: {}", read.description(project), first_line(error)),
        None => first_line(error),
    }
}

/// The last log line of every service that stopped instead of starting.
///
/// Only exited services are read: a healthy stack pays nothing for a
/// diagnostic it does not need, and an unhealthy one gets the evidence its
/// health check could not give.
///
/// Each read is a bounded runtime call of its own, and the readiness step has
/// already ended on one failure path, so the reads run under a live step that
/// names the container it is reading and reports a read the runtime does not
/// answer (effigy#460, Q-017).
fn exited_logs(
    repo_root: &Path,
    policy: &EffectiveContainerPolicy,
    services: &[ServiceHealth],
    parent: &ProgressSource,
) -> BTreeMap<String, String> {
    let exited = services
        .iter()
        .filter(|service| service.has_exited())
        .collect::<Vec<_>>();
    if exited.is_empty() {
        return BTreeMap::new();
    }
    let total = exited.len() as u64;
    let progress = exited_logs_progress(total);
    let step = TimedStep::live(progress.clone());
    let mut logs = BTreeMap::new();
    let mut unanswered = 0_u64;
    for (index, service) in exited.iter().enumerate() {
        let done = index as u64 + 1;
        // The read is issued against the runtime container, so the frames name
        // that identity — a Compose `container_name` may differ from the
        // service name (effigy#460, Q-017).
        let sub_step = exited_log_step_label(&service.container, done, total);
        progress.update(sub_step.clone(), Some((done, total)), None);
        // The readiness wait's own frame names the same read, so a timeout never
        // shows the finished probe while a log read runs.
        parent.now_waiting_for(exited_log_parent_label(&service.container));
        parent.update(sub_step, Some((done, total)), None);
        match container_last_log_line_for_policy(
            repo_root,
            policy,
            &service.container,
            EXITED_LOG_TAIL,
        ) {
            Ok(line) if !line.is_empty() => {
                logs.insert(service.service.clone(), line);
            }
            Ok(_) => {}
            Err(_) => unanswered += 1,
        }
    }
    if unanswered > 0 {
        step.fail(format!(
            "{unanswered} of {total} exited container log read{} did not answer",
            if total == 1 { "" } else { "s" }
        ));
    } else {
        step.succeed(format!(
            "read the last output of {total} exited container{}",
            if total == 1 { "" } else { "s" }
        ));
    }
    logs
}

/// The live step the exited-container log reads share: the count they cover and
/// elapsed time (effigy#460, Q-017).
fn exited_logs_progress(total: u64) -> ProgressSource {
    ProgressSource::waiting_for(format!(
        "the last output of {total} exited container{}",
        if total == 1 { "" } else { "s" }
    ))
}

/// One exited container's log read, with its position in the set. `container`
/// is the runtime identity the read is issued against, not the Compose service
/// name (effigy#460).
fn exited_log_step_label(container: &str, done: u64, total: u64) -> String {
    format!("reading the last line `{container}` printed ({done}/{total})")
}

/// The same read named on the readiness wait's own frame.
fn exited_log_parent_label(container: &str) -> String {
    format!("the runtime's log tail of `{container}`")
}

/// Update one step per service and return the ones that are not ready. A
/// waiting service's frame names its attempt and the last thing its health
/// check or container state reported.
///
/// A healthy service's step finishes and leaves the map, so a service whose
/// health regresses later opens a fresh step with its own attempt and last
/// check output instead of updating a step that already ended (effigy#460).
fn report_services(
    services: &[ServiceHealth],
    steps: &mut BTreeMap<String, ServiceStep>,
    attempts: &mut BTreeMap<String, u32>,
) -> Vec<String> {
    let mut not_ready = Vec::new();
    for service in services {
        if service.is_ready() {
            if let Some(waiting) = steps.remove(&service.service) {
                waiting
                    .step
                    .succeed(format!("{} is healthy", service.service));
            }
            // The next wait for this service starts its own attempt count.
            attempts.remove(&service.service);
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
        // A failed container-list read names that read and its probe number.
        let list = ProbeRead::containers(3);
        let waits = waiting_on_entries(
            &services,
            Some(&list),
            Some("docker: daemon not responding"),
            "demo",
        );
        assert_eq!(waits.len(), 2, "{waits:?}");
        assert_eq!(waits[0]["kind"], "service");
        assert_eq!(waits[0]["service"], "web");
        assert_eq!(
            waits[0]["problem"],
            "web is unhealthy after 4 failed checks"
        );
        assert_eq!(waits[1]["kind"], "probe");
        assert_eq!(waits[1]["project"], "demo");
        assert_eq!(waits[1]["probe"], "container_list");
        assert_eq!(waits[1]["probe_number"], 3);
        assert_eq!(waits[1]["error"], "docker: daemon not responding");
        assert!(
            waits[1].get("containers").is_none(),
            "a list read inspected no containers: {waits:?}"
        );

        // A failed inspect read carries the containers it was reading.
        let inspect = ProbeRead {
            probe: 4,
            stage: ProbeReadStage::Inspection(vec![
                "demo-web-1".to_owned(),
                "demo-db-1".to_owned(),
            ]),
        };
        let waits = waiting_on_entries(
            &services,
            Some(&inspect),
            Some("docker: daemon not responding"),
            "demo",
        );
        assert_eq!(waits[1]["probe"], "containers_inspect");
        assert_eq!(waits[1]["probe_number"], 4);
        assert_eq!(
            waits[1]["containers"],
            serde_json::json!(["demo-web-1", "demo-db-1"])
        );
    }

    #[test]
    fn the_probe_frame_names_the_runtime_read_in_flight_and_its_subjects() {
        let progress = ProgressSource::waiting_for("the environment's services to report healthy");
        observe_parent(
            &progress,
            &[
                health(
                    "web",
                    "running",
                    Some(HealthState::Unhealthy),
                    4,
                    Some("GET / HTTP/1.1 404"),
                ),
                health("db", "running", Some(HealthState::Healthy), 0, None),
            ],
        );
        // The probe is two runtime reads: the list, then the inspect of the
        // names it returned. Each frame names the one now in flight.
        observe_probe_stage(
            &progress,
            "demo",
            3,
            Duration::from_secs(15),
            Some((1, 2)),
            HealthProbeStage::ListingContainers,
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(20)),
            "listing the containers of project `demo` from the runtime (probe 3, up to 15s) (1/2); last: web is unhealthy after 4 failed checks; 20s elapsed"
        );
        let containers = ["demo-web-1".to_owned(), "demo-db-1".to_owned()];
        observe_probe_stage(
            &progress,
            "demo",
            4,
            Duration::from_secs(5),
            Some((1, 2)),
            HealthProbeStage::InspectingContainers {
                containers: &containers,
            },
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(24)),
            "inspecting 2 containers of project `demo`: `demo-web-1`, `demo-db-1` (probe 4, up to 5s) (1/2); last: web is unhealthy after 4 failed checks; 24s elapsed"
        );
        // A long list stays one bounded frame.
        let many = (0..5)
            .map(|index| format!("demo-{index}-1"))
            .collect::<Vec<_>>();
        observe_probe_stage(
            &progress,
            "demo",
            5,
            Duration::from_secs(5),
            None,
            HealthProbeStage::InspectingContainers { containers: &many },
        );
        assert!(
            progress
                .frame_at(Duration::from_secs(25))
                .contains("`demo-0-1`, `demo-1-1`, `demo-2-1` and 2 more"),
            "{}",
            progress.frame_at(Duration::from_secs(25))
        );
    }

    #[test]
    fn a_service_that_regresses_opens_a_fresh_step_with_its_own_attempt() {
        let mut steps = BTreeMap::new();
        let mut attempts = BTreeMap::new();
        let healthy = vec![
            health("web", "running", Some(HealthState::Healthy), 0, None),
            health(
                "db",
                "running",
                Some(HealthState::Starting),
                0,
                Some("waiting for connections"),
            ),
        ];
        let not_ready = report_services(&healthy, &mut steps, &mut attempts);
        assert_eq!(not_ready, vec!["db"]);
        assert!(
            !steps.contains_key("web"),
            "a healthy service's finished step leaves the map"
        );
        assert!(!attempts.contains_key("web"));

        // `web` regresses while `db` is still starting: the next observation
        // opens a new step with its own attempt and last check output.
        let regressed = vec![
            health(
                "web",
                "running",
                Some(HealthState::Unhealthy),
                1,
                Some("GET / HTTP/1.1 500"),
            ),
            health("db", "running", Some(HealthState::Healthy), 0, None),
        ];
        let not_ready = report_services(&regressed, &mut steps, &mut attempts);
        assert_eq!(not_ready, vec!["web"]);
        assert_eq!(attempts.get("web"), Some(&1));
        let waiting = steps.get("web").expect("the regressed service waits again");
        assert_eq!(
            waiting.progress.frame_at(Duration::from_secs(7)),
            "waiting for web to report healthy (attempt 1); last: GET / HTTP/1.1 500; 7s elapsed"
        );
        assert!(!steps.contains_key("db"));
    }

    #[test]
    fn a_health_read_in_flight_names_its_probe_and_keeps_the_last_known_state() {
        let progress = ProgressSource::waiting_for("the environment's services to report healthy");
        observe_parent(
            &progress,
            &[
                health(
                    "web",
                    "running",
                    Some(HealthState::Unhealthy),
                    4,
                    Some("GET / HTTP/1.1 404"),
                ),
                health("db", "running", Some(HealthState::Healthy), 0, None),
            ],
        );
        observe_probe_started(&progress, "demo", 3, Duration::from_secs(15), Some((1, 2)));
        assert_eq!(
            progress.frame_at(Duration::from_secs(20)),
            "reading the health of project `demo` from the runtime (probe 3, up to 15s) (1/2); last: web is unhealthy after 4 failed checks; 20s elapsed"
        );
    }

    #[test]
    fn a_silent_runtime_names_the_exact_health_read_it_waits_on() {
        let progress = ProgressSource::waiting_for("the environment's services to report healthy");
        observe_probe_failure(
            &progress,
            "demo",
            &ProbeRead::containers(3),
            "docker: daemon not responding\nsecond line",
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(30)),
            "waiting for the runtime to answer the container list of project `demo` (probe 3); last: docker: daemon not responding; 30s elapsed"
        );
        // An inspect failure names the containers it was reading, even when
        // the error text is identical to the list read's.
        observe_probe_failure(
            &progress,
            "demo",
            &ProbeRead {
                probe: 4,
                stage: ProbeReadStage::Inspection(vec![
                    "demo-web-1".to_owned(),
                    "demo-db-1".to_owned(),
                ]),
            },
            "docker: daemon not responding\nsecond line",
        );
        assert_eq!(
            progress.frame_at(Duration::from_secs(31)),
            "waiting for the runtime to answer the inspect of 2 containers of project `demo`: `demo-web-1`, `demo-db-1` (probe 4); last: docker: daemon not responding; 31s elapsed"
        );
    }

    #[test]
    fn a_failed_read_is_described_with_its_subjects_and_its_error() {
        let inspect = ProbeRead {
            probe: 5,
            stage: ProbeReadStage::Inspection(vec!["demo-web-1".to_owned()]),
        };
        assert_eq!(
            probe_read_error(
                "demo-web-renamed",
                Some(&inspect),
                "docker: daemon not responding\nsecond line",
            ),
            "the inspect of 1 container of project `demo-web-renamed`: `demo-web-1` (probe 5): docker: daemon not responding"
        );
        assert_eq!(
            probe_read_error(
                "demo-web-renamed",
                Some(&ProbeRead::containers(2)),
                "docker: daemon not responding",
            ),
            "the container list of project `demo-web-renamed` (probe 2): docker: daemon not responding"
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
    fn the_exited_log_reads_name_the_container_they_are_reading() {
        // The readiness step has already ended on one failure path, so these
        // reads own their live step; each frame names the container and its
        // position, and the readiness frame names the same read.
        let progress = exited_logs_progress(2);
        assert_eq!(
            progress.frame_at(Duration::from_secs(3)),
            "waiting for the last output of 2 exited containers; 3s elapsed"
        );
        let sub_step = exited_log_step_label("demo-web-1", 1, 2);
        progress.update(sub_step.clone(), Some((1, 2)), None);
        assert_eq!(
            progress.frame_at(Duration::from_secs(9)),
            "reading the last line `demo-web-1` printed (1/2) (1/2); waiting for the last output of 2 exited containers; 9s elapsed"
        );
        let parent = ProgressSource::waiting_for("the environment's services to report healthy");
        parent.now_waiting_for(exited_log_parent_label("demo-web-1"));
        parent.update(sub_step, Some((1, 2)), None);
        assert_eq!(
            parent.frame_at(Duration::from_secs(12)),
            "reading the last line `demo-web-1` printed (1/2) (1/2); waiting for the runtime's log tail of `demo-web-1`; 12s elapsed"
        );
    }

    /// A checkout whose runtime answers one log read, inside a fixture
    /// directory, so the exited-service diagnostic can be exercised without a
    /// real container runtime.
    fn exited_logs_fixture(
        name: &str,
        logs_answer: &str,
    ) -> (
        effigy_test_support::Fixture,
        EffectiveContainerPolicy,
        crate::contract_test_support::EnvGuard,
    ) {
        use std::os::unix::fs::PermissionsExt;
        let fixture = effigy_test_support::Fixture::workspace(name).expect("fixture workspace");
        let root = fixture.path().canonicalize().unwrap();
        std::fs::write(
            root.join("effigy.toml"),
            "[containers]\ndefault = \"web\"\n\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n",
        )
        .expect("write manifest");
        std::fs::write(
            root.join("compose.yml"),
            "services:\n  app:\n    image: alpine:latest\n",
        )
        .expect("write compose");
        let bin = root.join("bin");
        std::fs::create_dir(&bin).expect("mkdir bin");
        let log = root.join("calls");
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> {log}\ncase \"$*\" in\n  logs*) {logs_answer} ;;\n  *) exit 0 ;;\nesac\n",
            log = log.display(),
        );
        for name in ["docker", "colima"] {
            let executable = bin.join(name);
            std::fs::write(&executable, &script).expect("write fake runtime");
            std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o700))
                .expect("chmod fake runtime");
        }
        let env = crate::contract_test_support::EnvGuard::set_many(&[
            (
                "PATH",
                Some(format!(
                    "{}:{}",
                    bin.display(),
                    std::env::var("PATH").unwrap()
                )),
            ),
            ("HOME", Some(root.display().to_string())),
            ("EFFIGY_COMPOSE_BACKEND", Some("docker".into())),
        ]);
        let (policy, _decisions) =
            effigy_containers::load_container_policy_for_creation(&root, Some("web"))
                .expect("creation policy");
        (fixture, policy, env)
    }

    #[test]
    fn a_readiness_timeout_names_each_exited_log_read_and_its_evidence() {
        let (fixture, policy, _env) = exited_logs_fixture(
            "effigy-readiness-exited-logs-",
            "printf 'app: database is not ready\\n'",
        );
        // The Compose service is `db`, but its runtime container carries an
        // explicit `container_name`: the log read and both frames name the
        // container, which is what the read is issued against.
        let mut exited = health("db", "exited", None, 0, None);
        exited.container = "custom-container".to_owned();
        let services = vec![exited];
        let parent = ProgressSource::waiting_for("the environment's services to report healthy");
        let logs = exited_logs(&policy.repo_root, &policy, &services, &parent);
        assert_eq!(
            logs.get("db").map(String::as_str),
            Some("app: database is not ready"),
            "the diagnostic is keyed by the service its log line belongs to"
        );
        let calls = std::fs::read_to_string(fixture.path().join("calls")).unwrap_or_default();
        assert!(
            calls
                .lines()
                .any(|call| call == "logs --tail 20 custom-container"),
            "the read targets the runtime container: {calls:?}"
        );
        assert_eq!(
            parent.frame_at(Duration::from_secs(5)),
            "reading the last line `custom-container` printed (1/1) (1/1); waiting for the runtime's log tail of `custom-container`; 5s elapsed",
            "both readiness frames name the container, not the service"
        );
    }

    #[test]
    fn a_readiness_timeout_without_a_log_answer_still_reports_the_service() {
        let (_fixture, policy, _env) = exited_logs_fixture(
            "effigy-readiness-exited-logs-missing-",
            "echo 'runtime logs unavailable' >&2; exit 1",
        );
        let services = vec![health("db", "exited", None, 0, None)];
        let parent = ProgressSource::waiting_for("the environment's services to report healthy");
        let logs = exited_logs(&policy.repo_root, &policy, &services, &parent);
        assert!(
            logs.is_empty(),
            "an unanswered log read yields no evidence, not a guess"
        );
        // `health()` names the container `demo-<service>-1`; the unanswered read
        // named that container too.
        assert_eq!(
            parent.frame_at(Duration::from_secs(4)),
            "reading the last line `demo-db-1` printed (1/1) (1/1); waiting for the runtime's log tail of `demo-db-1`; 4s elapsed"
        );
        // The evidence it does have still reaches the diagnostic.
        let parent_step = TimedStep::live(parent.clone());
        let Err(error) = readiness_failure(
            &policy,
            &services,
            None,
            None,
            &BTreeMap::new(),
            &parent_step,
            &parent,
        ) else {
            panic!("a readiness failure is an error");
        };
        let message = error.to_string();
        assert!(message.contains("db exited"), "{message}");
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
    /// A readiness report with the given end and no observed services.
    fn report(end: ReadinessEnd) -> Result<ReadinessReport, RunnerError> {
        Ok(ReadinessReport {
            end,
            observed: Vec::new(),
        })
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
        end_environment_phase(&ledger, &report(ReadinessEnd::Ready));
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
        end_environment_phase(&ledger, &report(ReadinessEnd::Interrupted));
        assert_eq!(ledger.read_progress(), None);
        ledger.publish_phase(&phase_counts(3, 4));
        end_environment_phase(&ledger, &report(ReadinessEnd::NothingToAwait));
        assert_eq!(ledger.read_progress(), None);
    }
}
