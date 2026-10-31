//! The environment phase of a managed run: intent, `up` under custody, then
//! readiness observed against deadlines from the run (papercut 2abfa556).
//!
//! Nothing waits on a fixed timer. Each wait observes its condition, starts
//! its deadline when its own phase starts, and reports what it saw. Exec
//! workloads start only once the environment is ready, so they carry no
//! readiness loop of their own, and nothing here installs a shell trap.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use effigy_kernel::ctx::{OpenReason, StopCause};
use effigy_kernel::custody::{EnvValue, Lifetime, ProcessSpec, Terminal};
use effigy_kernel::observe::Observed;
use effigy_kernel::probe::ProbeCommand;
use effigy_kernel::run::{EffectSpec, Settlement};

use super::settlement::{self, Binding, EnvironmentEffect};
use crate::runner::task_engine::{self, TaskRun};

/// Poll interval for a custody child and between readiness attempts.
const POLL: Duration = Duration::from_millis(50);
/// How often a still-starting `up` reports progress to the console.
const START_PROGRESS_INTERVAL: Duration = Duration::from_secs(30);
/// Readiness retries back off to at most this.
const BACKOFF_MAX: Duration = Duration::from_secs(5);
/// One route probe's own bound.
const ROUTE_PROBE: Duration = Duration::from_secs(3);
/// How often the environment phase re-reads the container counts its own `up`
/// publishes, so every entry that waits for it names them (effigy#458).
const ENVIRONMENT_PROGRESS_READ: Duration = Duration::from_millis(250);

/// The environment one managed run brings up, or borrows.
#[derive(Debug, Clone)]
pub(in crate::runner) struct EnvironmentPlan {
    pub(in crate::runner) name: String,
    pub(in crate::runner) repo: PathBuf,
    pub(in crate::runner) binding: Binding,
    /// The shell command that brings the environment up and returns.
    pub(in crate::runner) up: String,
    pub(in crate::runner) up_env: Vec<(String, String)>,
    /// Already running for its own owner before this run: retained at settle.
    pub(in crate::runner) borrowed: bool,
    /// The service exec workloads attach to.
    pub(in crate::runner) exec_probe: Option<String>,
    /// `health_wait` routes, probed once the workloads have started.
    pub(in crate::runner) route_urls: Vec<String>,
    /// Bounds each readiness phase from the moment it starts.
    pub(in crate::runner) readiness_budget: Duration,
    /// Lines printed once the environment is ready.
    pub(in crate::runner) ready_lines: Vec<String>,
}

/// Why the environment phase ended without a ready environment.
#[derive(Debug)]
pub(in crate::runner) enum NotReady {
    Stopped(StopCause),
    Failed(String),
}

/// Brings the environment up, records the run's obligation to stop it (or
/// leave a borrowed one running) and observes its readiness. `stop` is
/// polled at every wait boundary.
///
/// `progress` receives the containers the environment's own `up` reports
/// healthy while the phase runs, so the wait names them rather than a generic
/// spinner.
pub(in crate::runner) fn bring_up(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    token: &str,
    ansi_front: bool,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
    progress: &mut dyn FnMut(Option<(u32, u32)>),
) -> Result<(), NotReady> {
    let effect = EnvironmentEffect {
        repo: plan.repo.clone(),
        environment: plan.name.clone(),
        binding: plan.binding.clone(),
        token: token.to_owned(),
        borrowed: plan.borrowed,
        run: Some(run.id().to_string()),
    };
    println!(
        "[environment] {} `{}`: bringing it up",
        if plan.borrowed {
            "borrowing"
        } else {
            "starting"
        },
        plan.name
    );
    // The bring-up is the reconciler's own journalled effect, and `up` runs
    // as this run's custody child. What this run owes from here on, whether
    // or not `up` finished, is the environment's stop at its settle, or, for
    // a borrowed environment, leaving it running. That obligation is recorded
    // after the activation's attachment, so the reverse-order settle (K4)
    // stops the environment before it releases the attachment, which the
    // kernel releases only once nothing else of the run is open.
    let up = run_up(run, plan, ansi_front, stop, progress);
    let ticket = {
        let mut kernel = run
            .run_mut()
            .map_err(|error| NotReady::Failed(error.to_string()))?;
        let target = kernel.fence().clone();
        kernel
            .intend(EffectSpec {
                target,
                kind: settlement::ENVIRONMENT.to_owned(),
                payload: serde_json::to_value(&effect)
                    .map_err(|error| NotReady::Failed(error.to_string()))?,
                settlement: if plan.borrowed {
                    Settlement::Retain
                } else {
                    Settlement::Release(serde_json::Value::Null)
                },
                required: false,
            })
            .map_err(|error| NotReady::Failed(format!("environment intent: {error}")))?
    };
    let receipt = match &up {
        Ok(code) => serde_json::json!({ "up": code }),
        Err(NotReady::Failed(reason)) => serde_json::json!({ "up_failed": reason }),
        Err(NotReady::Stopped(_)) => serde_json::json!({ "up_stopped": true }),
    };
    run.run_mut()
        .map_err(|error| NotReady::Failed(error.to_string()))?
        .applied(&ticket, receipt)
        .map_err(|error| NotReady::Failed(format!("environment receipt: {error}")))?;
    up?;
    if let Some(service) = &plan.exec_probe {
        exec_ready(run, plan, service, stop)?;
    }
    Ok(())
}

/// `health_wait` after the workloads start: the routes they serve can answer
/// only then. The owner loop polls it one probe at a time, so frames, exits
/// and stops keep flowing while it waits.
pub(in crate::runner) struct RouteHealth {
    urls: Vec<String>,
    ready_lines: Vec<String>,
    budget: Duration,
    started: Option<Instant>,
    next_at: Instant,
    backoff: Duration,
    last: String,
}

/// One poll of [`RouteHealth`].
pub(in crate::runner) enum Health {
    Pending,
    Ready,
    Failed(String),
}

impl RouteHealth {
    pub(in crate::runner) fn new(plan: &EnvironmentPlan) -> Self {
        Self {
            urls: plan.route_urls.clone(),
            ready_lines: plan.ready_lines.clone(),
            budget: plan.readiness_budget,
            started: None,
            next_at: Instant::now(),
            backoff: Duration::from_millis(250),
            last: String::new(),
        }
    }

    /// The request the wait is currently observing, named the way a person
    /// reads it: `GET /api/health`.
    pub(in crate::runner) fn probe_label(&self) -> Option<String> {
        let url = self.urls.first()?;
        let path = url
            .split_once("://")
            .map_or(url.as_str(), |(_scheme, rest)| rest);
        let path = path.find('/').map_or("/", |slash| &path[slash..]);
        Some(format!("GET {path}"))
    }

    /// Probes the next unready route when its backoff is due. The deadline
    /// starts at the first poll, once every workload has started.
    pub(in crate::runner) fn poll(&mut self) -> Health {
        let now = Instant::now();
        let started = *self.started.get_or_insert(now);
        if now < self.next_at {
            return Health::Pending;
        }
        while let Some(url) = self.urls.first() {
            let observed = match find_program("curl") {
                Some(curl) => probe_route(&curl, url),
                None => {
                    return Health::Failed(
                        "`health_wait` needs `curl` on PATH to probe routes".to_owned(),
                    );
                }
            };
            match observed {
                Ok(()) => {
                    println!(
                        "[environment] ready: route {url} after {}s",
                        started.elapsed().as_secs()
                    );
                    self.urls.remove(0);
                    self.backoff = Duration::from_millis(250);
                    self.last.clear();
                }
                Err(observed) => {
                    if observed != self.last {
                        println!("[environment] waiting: route {url}: {observed}");
                        self.last = observed;
                    }
                    if started.elapsed() >= self.budget {
                        return Health::Failed(format!(
                            "route {url} was not ready within {}s of the workloads starting; \
                             last observed: {}",
                            self.budget.as_secs(),
                            self.last
                        ));
                    }
                    self.next_at = Instant::now() + self.backoff;
                    self.backoff = (self.backoff * 2).min(BACKOFF_MAX);
                    return Health::Pending;
                }
            }
        }
        for line in self.ready_lines.drain(..) {
            println!("[environment] {line}");
        }
        Health::Ready
    }
}

/// `up` as a custody child of the run: its output is the run's output, and
/// the run's settle stops it if the supervisor dies inside it.
fn run_up(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    ansi_front: bool,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
    progress: &mut dyn FnMut(Option<(u32, u32)>),
) -> Result<i32, NotReady> {
    let mut env = task_engine::spawn_environment(&plan.repo)
        .map_err(|error| NotReady::Failed(error.to_string()))?;
    for (key, value) in &plan.up_env {
        env.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    env.insert(
        crate::runner::managed_color::CAPABILITY.into(),
        EnvValue::Plain(if ansi_front { "1" } else { "0" }.into()),
    );
    // The bring-up's activation attaches this run as the environment's
    // holder (contract 053 §4); a named environment's ensure is then held.
    if matches!(plan.binding, Binding::Named { .. }) {
        env.insert(
            OsString::from(crate::runner::environment::HOLDER_ENV),
            EnvValue::Plain(OsString::from(run.id().to_string())),
        );
    }
    // `up` is Effigy's own child, so it receives this session's vault access
    // through the one-shot descriptor handoff rather than its environment:
    // the passphrase is never in argv, never in a shell string, and never in
    // the environment a workload inherits (effigy#338).
    let grant = crate::runner::secret_session::take_child_grant();
    if let Some(grant) = &grant {
        let (key, value) = grant.env_entry();
        env.insert(OsString::from(key), EnvValue::Plain(OsString::from(value)));
    }
    let spec = ProcessSpec {
        argv: vec!["/bin/sh".into(), "-c".into(), plan.up.clone().into()],
        cwd: plan.repo.clone(),
        env,
        grace: Duration::from_secs(5),
        // `up` is its own two-phase wait: the bring-up's readiness answers it,
        // or `up` itself fails with the bounds it declares. A run-level
        // deadline here would cap a slow first build at a number nothing in the
        // manifest controls (effigy#370).
        lifetime: Lifetime::Open(OpenReason::DevServer),
        terminal: Terminal::Inherit { capture: false },
        label: "environment up".to_owned(),
    };
    let started = Instant::now();
    let mut process = run
        .spawn_with_handoff(spec, false, grant.map(|grant| grant.into_reader()))
        .map_err(|error| NotReady::Failed(format!("environment up did not start: {error}")))?;
    let mut next_progress = started + START_PROGRESS_INTERVAL;
    let mut next_progress_read = started;
    loop {
        run.verify_admission()
            .map_err(|error| NotReady::Failed(error.to_string()))?;
        if let Some(cause) = stop(run) {
            return Err(NotReady::Stopped(cause));
        }
        if Instant::now() >= next_progress_read {
            next_progress_read = Instant::now() + ENVIRONMENT_PROGRESS_READ;
            progress(environment_counts(plan));
        }
        if Instant::now() >= next_progress {
            next_progress = Instant::now() + START_PROGRESS_INTERVAL;
            println!(
                "[environment] `{}` is still coming up after {}s; its `up` step is still running, so nothing has timed out",
                plan.name,
                started.elapsed().as_secs()
            );
        }
        if let Observed::Verified(exit, _) = process.poll() {
            let _ = process.complete(
                &mut *run
                    .run_mut()
                    .map_err(|error| NotReady::Failed(error.to_string()))?,
                exit,
            );
            if exit.code != 0 {
                return Err(NotReady::Failed(format!(
                    "environment `{}` did not come up: `up` exited {} after {}s",
                    plan.name,
                    exit.code,
                    started.elapsed().as_secs()
                )));
            }
            println!(
                "[environment] `{}` is up after {}s",
                plan.name,
                started.elapsed().as_secs()
            );
            return Ok(exit.code);
        }
        std::thread::sleep(POLL);
    }
}

/// The containers the environment's own `up` reported healthy so far, from
/// the record it publishes while it waits for readiness.
fn environment_counts(plan: &EnvironmentPlan) -> Option<(u32, u32)> {
    // The container readiness publisher writes the phase's own record, not an
    // entry's, and a host process may legitimately be named `environment`
    // (effigy#458).
    crate::runner::managed_wait::WaitLedger::environment(&plan.repo, &plan.name).read_progress()
}

/// The owned binding of the service exec workloads attach to, observed with
/// backoff until its own deadline. Each attempt's result is reported.
fn exec_ready(
    run: &mut TaskRun,
    plan: &EnvironmentPlan,
    service: &str,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
) -> Result<(), NotReady> {
    let policy = match settlement::policy(&plan.repo, &plan.binding) {
        Ok(policy) => policy,
        Err(reason) => return Err(NotReady::Failed(reason)),
    };
    let deadline = Instant::now() + plan.readiness_budget;
    observe_until(
        run,
        &format!("exec into `{service}`"),
        deadline,
        stop,
        &mut |attempt_deadline| match crate::runner::container_runtime_prep::verify_service_binding(
            &plan.repo,
            &policy,
            service,
            Some(attempt_deadline),
        ) {
            Ok(_) => Ok(()),
            Err(error) => Err(first_line(&error.to_string())),
        },
    )
}

/// One `health_wait` route probe: any status other than a gateway or
/// connection failure (000, 502, 503, 504), run under a probe guard.
fn probe_route(curl: &Path, url: &str) -> Result<(), String> {
    let observe = task_engine::observe_context(ROUTE_PROBE + Duration::from_secs(1));
    let output = observe.probe_process(ProbeCommand {
        argv: vec![
            curl.into(),
            "-k".into(),
            "-s".into(),
            "-o".into(),
            "/dev/null".into(),
            "-w".into(),
            "%{http_code}".into(),
            "--max-time".into(),
            ROUTE_PROBE.as_secs().to_string().into(),
            url.into(),
        ],
        cwd: PathBuf::from("/"),
        // curl needs only these; the kernel refuses its reserved keys.
        env: ["PATH", "HOME"]
            .into_iter()
            .filter_map(|key| {
                std::env::var_os(key).map(|value| (OsString::from(key), EnvValue::Plain(value)))
            })
            .collect::<BTreeMap<_, _>>(),
        deadline: ROUTE_PROBE + Duration::from_millis(500),
        max_output: 64,
    });
    match output {
        Observed::Verified(output, _) => {
            let code = String::from_utf8_lossy(&output.output).trim().to_owned();
            match code.as_str() {
                "" | "000" | "502" | "503" | "504" => Err(format!(
                    "answered {}",
                    if code.is_empty() { "nothing" } else { &code }
                )),
                _ => Ok(()),
            }
        }
        Observed::Absent(_) => Err("probe produced no observation".to_owned()),
        Observed::Unknown(reason) => Err(format!("unobservable: {reason}")),
    }
}

/// Retries `attempt` until it succeeds, `deadline` passes or the run stops,
/// reporting each new observation once.
fn observe_until(
    run: &mut TaskRun,
    what: &str,
    deadline: Instant,
    stop: &mut dyn FnMut(&mut TaskRun) -> Option<StopCause>,
    attempt: &mut dyn FnMut(Instant) -> Result<(), String>,
) -> Result<(), NotReady> {
    let started = Instant::now();
    let mut backoff = Duration::from_millis(250);
    let mut last = String::new();
    loop {
        if let Some(cause) = stop(run) {
            return Err(NotReady::Stopped(cause));
        }
        let attempt_deadline = deadline.min(Instant::now() + Duration::from_secs(15));
        match attempt(attempt_deadline) {
            Ok(()) => {
                println!(
                    "[environment] ready: {what} after {}s",
                    started.elapsed().as_secs()
                );
                return Ok(());
            }
            Err(observed) => {
                if observed != last {
                    println!("[environment] waiting: {what}: {observed}");
                    last = observed;
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(NotReady::Failed(format!(
                "{what} was not ready within {}s of the environment coming up; last observed: {last}",
                deadline.saturating_duration_since(started).as_secs()
            )));
        }
        let wake = Instant::now() + backoff;
        while Instant::now() < wake.min(deadline) {
            if let Some(cause) = stop(run) {
                return Err(NotReady::Stopped(cause));
            }
            std::thread::sleep(POLL);
        }
        backoff = (backoff * 2).min(BACKOFF_MAX);
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_owned()
}

fn find_program(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_tui::waits::WaitOn;

    /// The wait names the request being observed, not the whole URL: the
    /// readiness line a person reads (effigy#458).
    #[test]
    fn the_readiness_probe_names_its_request() {
        let route = health(
            "http://app.test/api/health".to_owned(),
            Duration::from_secs(1),
        );
        assert_eq!(route.probe_label().as_deref(), Some("GET /api/health"));
        let route = health("http://app.test".to_owned(), Duration::from_secs(1));
        assert_eq!(route.probe_label().as_deref(), Some("GET /"));
        assert_eq!(
            RouteHealth::new(&plan(std::env::temp_dir())).probe_label(),
            None
        );
    }

    /// The environment phase reads the containers its own `up` reported
    /// healthy from the record that `up` publishes.
    /// The owner reads the counts from the record the readiness publisher
    /// writes (`publish_phase`), and a host process an operator named
    /// `environment` cannot shadow the phase (effigy#458).
    #[test]
    fn the_environment_phase_reads_the_published_container_counts() {
        let repo = tempfile::tempdir().expect("repo");
        let plan = plan(repo.path().to_path_buf());
        let ledger = crate::runner::managed_wait::WaitLedger::environment(repo.path(), &plan.name);
        ledger.clear();
        assert_eq!(environment_counts(&plan), None, "nothing is published yet");

        // The readiness publisher's own write path.
        ledger.publish_phase(&effigy_tui::waits::EntryState {
            entry: "environment".to_owned(),
            phase: effigy_tui::waits::EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 12_000,
        });
        assert_eq!(environment_counts(&plan), Some((3, 4)));

        // A host process that happens to be named `environment` is an entry,
        // not the phase, and never changes the counts.
        ledger.publish(&effigy_tui::waits::EntryState {
            entry: "environment".to_owned(),
            phase: effigy_tui::waits::EntryPhase::Starting,
            waiting_on: Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(environment_counts(&plan), Some((3, 4)));

        // The phase reporting ready ends the counts.
        ledger.settle_phase();
        assert_eq!(environment_counts(&plan), None);
    }

    fn plan(repo: std::path::PathBuf) -> EnvironmentPlan {
        EnvironmentPlan {
            name: "app".to_owned(),
            repo,
            binding: Binding::Inline {
                task: "dev".to_owned(),
            },
            up: "true".to_owned(),
            up_env: Vec::new(),
            borrowed: false,
            exec_probe: None,
            route_urls: Vec::new(),
            readiness_budget: Duration::from_secs(60),
            ready_lines: Vec::new(),
        }
    }

    #[test]
    fn first_line_keeps_only_the_headline() {
        assert_eq!(first_line("not ready\nmore"), "not ready");
        assert_eq!(first_line(""), "");
    }

    fn health(url: String, budget: Duration) -> RouteHealth {
        RouteHealth {
            urls: vec![url],
            ready_lines: vec!["ready: http://app.test".to_owned()],
            budget,
            started: None,
            next_at: Instant::now(),
            backoff: Duration::from_millis(10),
            last: String::new(),
        }
    }

    /// A route answers 503 until `serving` is set, then 200.
    fn route(serving: std::sync::Arc<std::sync::atomic::AtomicBool>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind route");
        let address = listener.local_addr().expect("route address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0u8; 1024];
                let _ = stream.read(&mut request);
                let ready = serving.load(std::sync::atomic::Ordering::Acquire);
                let status = if ready {
                    "200 OK"
                } else {
                    "503 Service Unavailable"
                };
                let _ = stream.write_all(
                    format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                        .as_bytes(),
                );
                if ready {
                    return;
                }
            }
        });
        format!("http://{address}/")
    }

    /// A route the workloads serve answers only once they run: health stays
    /// pending without blocking, then turns ready (the deadlock that kept
    /// consumers on `health_wait = false`).
    #[test]
    fn route_health_waits_for_a_route_its_workloads_serve() {
        let serving = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut health = health(route(serving.clone()), Duration::from_secs(30));
        let polled = Instant::now();
        assert!(matches!(health.poll(), Health::Pending));
        assert!(polled.elapsed() < ROUTE_PROBE + Duration::from_secs(1));
        assert!(health.last.contains("503"), "{}", health.last);
        serving.store(true, std::sync::atomic::Ordering::Release);
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match health.poll() {
                Health::Ready => break,
                Health::Pending => assert!(Instant::now() < deadline, "never ready"),
                Health::Failed(reason) => panic!("{reason}"),
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(health.ready_lines.is_empty(), "ready lines print once");
    }

    #[test]
    fn route_health_fails_at_its_budget_with_what_it_saw() {
        let serving = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut health = health(route(serving), Duration::ZERO);
        let Health::Failed(reason) = health.poll() else {
            panic!("a spent budget fails");
        };
        assert!(reason.contains("503"), "{reason}");
    }
}
