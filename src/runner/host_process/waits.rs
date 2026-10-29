//! The resolved host-process ordering graph, published while one bring-up
//! holds it (effigy#458).
//!
//! A host process's wait is not visible in tab order: `front` waits for the
//! reader its `depends_on` declares, and `api` waits for its own declared
//! readiness request. The bring-up therefore derives the whole graph once,
//! publishes every entry's wait before the first process can block, and
//! advances it as each dependency is observed. The launcher is a seam: the
//! kernel owns it in production, and a strict fake owns it in the tests, so the
//! graph, the published records and the spawn decisions under test are the
//! production ones.

use std::collections::BTreeSet;
use std::time::Instant;

use effigy_containers::{EffectiveContainerPolicy, EffectiveHostProcess};
use effigy_tui::waits::{should_publish, EntryState, EntryWaits, HostOrder, SessionOrder};

use std::collections::BTreeMap;

use super::ManagedHostListenerResult;
use crate::runner::error::RunnerError;
use crate::runner::managed_wait::{self, WaitLedger};

/// What one bring-up performed, once its whole plan ran.
#[derive(Debug, Default)]
pub(in crate::runner) struct HostPlanOutcome {
    pub(in crate::runner) listeners: Vec<ManagedHostListenerResult>,
}

/// The published waits of one host-process bring-up.
pub(in crate::runner) struct HostWaitGraph {
    ledger: WaitLedger,
    waits: EntryWaits,
    /// The last state published per entry, and when, so a settled record is
    /// written once and a live wait refreshes without a write per call.
    published: BTreeMap<String, EntryState>,
    published_at: BTreeMap<String, Instant>,
}

impl HostWaitGraph {
    /// The graph as the run's owner publishes it before the environment phase
    /// can block: the ledger is this session's, every declared host process
    /// waits for the environment, and nothing has started. The run's owner
    /// publishes it here so `status`, the tabs and the headless text name the
    /// host waits throughout bring-up, not only once the planner runs
    /// (effigy#458).
    pub(in crate::runner) fn for_environment_phase(
        repo_root: &std::path::Path,
        policy: &EffectiveContainerPolicy,
        now: Instant,
    ) -> Self {
        let ledger = WaitLedger::environment(repo_root, &policy.name);
        ledger.clear();
        let started = policy
            .host_processes
            .iter()
            .map(|process| process.name.clone())
            .collect::<BTreeSet<_>>();
        let mut graph = Self::build(&ledger, policy, &started, Some(&policy.name), now);
        graph.publish_all(now);
        graph
    }

    /// The graph of `policy`'s host processes. `started` names what this call
    /// replaces; every other declared process is already running, so it counts
    /// as ready and a dependent never waits for an entry this call does not
    /// start.
    pub(in crate::runner) fn for_policy(
        repo_root: &std::path::Path,
        policy: &EffectiveContainerPolicy,
        started: &BTreeSet<String>,
        now: Instant,
    ) -> Self {
        // The planner runs after the environment reported ready.
        Self::build(
            &WaitLedger::environment(repo_root, &policy.name),
            policy,
            started,
            None,
            now,
        )
    }

    fn build(
        ledger: &WaitLedger,
        policy: &EffectiveContainerPolicy,
        started: &BTreeSet<String>,
        environment: Option<&str>,
        now: Instant,
    ) -> Self {
        let order = SessionOrder::hosts(
            &policy
                .host_processes
                .iter()
                .map(|process| HostOrder {
                    name: &process.name,
                    depends_on: &process.depends_on,
                })
                .collect::<Vec<_>>(),
        );
        let mut waits = EntryWaits::new(order, environment, now);
        for process in &policy.host_processes {
            if !started.contains(&process.name) {
                waits.ready(&process.name, now);
            }
        }
        Self {
            ledger: ledger.clone(),
            waits,
            published: BTreeMap::new(),
            published_at: BTreeMap::new(),
        }
    }

    /// Publishes every entry's own wait: a waiting entry names its dependency
    /// and that dependency's phase, a starting listener names its readiness
    /// request, and a settled entry says so. Only a changed meaning or a live
    /// wait's refresh writes a record (effigy#458).
    pub(in crate::runner) fn publish_all(&mut self, now: Instant) {
        self.waits.refresh(now);
        for state in self.waits.states(now) {
            let due = should_publish(
                self.published.get(&state.entry),
                self.published_at.get(&state.entry).copied(),
                &state,
                now,
            );
            if !due {
                continue;
            }
            if state.holds_a_wait() {
                self.ledger.publish(&state);
            } else {
                self.ledger.settle(&state.entry);
            }
            self.published_at.insert(state.entry.clone(), now);
            self.published.insert(state.entry.clone(), state);
        }
    }

    /// The environment phase reported the containers that are healthy now.
    pub(in crate::runner) fn environment_progress(
        &mut self,
        healthy: u32,
        total: u32,
        now: Instant,
    ) {
        self.waits.environment_progress(healthy, total);
        self.publish_all(now);
    }

    /// The environment reported ready: the host entries now wait on their own
    /// `depends_on` readers instead of the phase.
    pub(in crate::runner) fn environment_ready(&mut self, now: Instant) {
        self.waits.environment_ready(now);
        self.publish_all(now);
    }

    /// The environment failed: no host process starts, and each one says so
    /// with the environment's own report.
    pub(in crate::runner) fn environment_failed(&mut self, detail: &str, now: Instant) {
        self.waits.environment_failed(detail, now);
        self.publish_all(now);
    }

    /// The run settled: every wait this graph published is over.
    pub(in crate::runner) fn settle_all(&mut self, now: Instant) {
        let entries = self.waits.states(now);
        for state in entries {
            if !matches!(
                state.phase,
                effigy_tui::waits::EntryPhase::NotStarted | effigy_tui::waits::EntryPhase::Failed
            ) {
                self.ledger.settle(&state.entry);
                self.published.remove(&state.entry);
                self.published_at.remove(&state.entry);
            }
        }
    }

    pub(in crate::runner) fn dependency_ready(&mut self, entry: &str, now: Instant) {
        self.waits.ready(entry, now);
        self.publish_all(now);
    }

    pub(in crate::runner) fn started(&mut self, entry: &str, now: Instant) {
        self.waits.started(entry, now);
        self.publish_all(now);
    }

    pub(in crate::runner) fn probing(&mut self, entry: &str, probe: String, now: Instant) {
        self.waits.probing(entry, probe, now);
        self.publish_all(now);
    }

    pub(in crate::runner) fn ready(&mut self, entry: &str, now: Instant) {
        self.waits.ready(entry, now);
        self.publish_all(now);
    }

    /// One entry's current state.
    pub(in crate::runner) fn state(&self, entry: &str, now: Instant) -> Option<EntryState> {
        self.waits.state(entry, now)
    }

    /// The entry failed: it is terminal with what it reported, and every
    /// dependent is `not started` with that attribution. Returns the entries
    /// that may no longer start.
    pub(in crate::runner) fn failed(
        &mut self,
        entry: &str,
        detail: &str,
        now: Instant,
    ) -> Vec<String> {
        self.waits.failed(entry, detail, now);
        self.publish_all(now);
        self.waits
            .not_started()
            .into_iter()
            .map(str::to_owned)
            .collect()
    }
}

/// What the plan asks of whoever runs it. The kernel launcher waits for real
/// listeners and starts real supervisors; a fake records spawns and answers
/// readiness from a script.
pub(in crate::runner) trait HostLauncher {
    /// Waits for `dependency`'s published listener readiness, reporting
    /// progress so the wait it holds can be published while it blocks.
    fn dependency_ready(
        &mut self,
        dependency: &str,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<(), RunnerError>;

    /// Launches `process` and, when it declares a listener, waits for its own
    /// declared readiness. Returns the listener result it observed.
    fn launch(
        &mut self,
        process: &str,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<Option<ManagedHostListenerResult>, RunnerError>;

    /// The clients this bring-up started, for the caller's detach and its
    /// owner-loss fallback.
    fn started(&self) -> &[(String, effigy_kernel::supervise::SupervisorClient)];

    /// Takes the clients out: dropping them without a detach is the kernel's
    /// owner-loss stop.
    fn take_started(&mut self) -> Vec<(String, effigy_kernel::supervise::SupervisorClient)>;
}

/// Runs the resolved plan in dependency order, publishing the graph as it goes.
///
/// The whole graph is published before the first process can block, so a
/// dependent's tab names its prerequisite while that prerequisite is still
/// becoming ready. A failure stops the plan with the failing entry's report;
/// every dependent is published `not started` and none is launched.
pub(in crate::runner) fn run_host_plan(
    ordered: &[&EffectiveHostProcess],
    selection: Option<&BTreeSet<String>>,
    graph: &mut HostWaitGraph,
    launcher: &mut dyn HostLauncher,
    now: &dyn Fn() -> Instant,
) -> Result<HostPlanOutcome, RunnerError> {
    graph.publish_all(now());
    let mut outcome = HostPlanOutcome::default();
    for process in ordered {
        if selection.is_some_and(|selection| !selection.contains(&process.name)) {
            continue;
        }
        // The entry whose failure ends the plan: the dependency being awaited,
        // or the process itself once its own start is what fails.
        // A blocked entry never starts: its prerequisite failed, so the plan
        // ends with the attributed result instead of launching a process that
        // cannot be what the graph promised (effigy#458).
        if let Some(state) = graph.state(&process.name, now()) {
            if matches!(
                state.phase,
                effigy_tui::waits::EntryPhase::NotStarted | effigy_tui::waits::EntryPhase::Failed
            ) {
                return Err(RunnerError::task_invocation(state.status_line()));
            }
        }
        let mut awaited = process.name.clone();
        let step = (|| -> Result<(), RunnerError> {
            for dependency in &process.depends_on {
                awaited = dependency.clone();
                graph.publish_all(now());
                launcher.dependency_ready(dependency, &mut |at| graph.publish_all(at))?;
                graph.dependency_ready(dependency, now());
            }
            awaited = process.name.clone();
            graph.started(&process.name, now());
            if let Some(listener) = &process.listener {
                graph.probing(
                    &process.name,
                    effigy_tui::waits::listener_probe_label(
                        &listener.readiness_path,
                        listener.readiness_status,
                    ),
                    now(),
                );
            }
            let listener = launcher.launch(&process.name, &mut |at| graph.publish_all(at))?;
            graph.ready(&process.name, now());
            outcome.listeners.extend(listener);
            Ok(())
        })();
        if let Err(error) = step {
            let attribution = managed_wait::failure_attribution(&awaited, &error);
            graph.failed(&awaited, &attribution.detail, now());
            return Err(error);
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    //! The deterministic four-entry graph the brief names, driven through the
    //! production plan: one environment, `api` with a declared readiness
    //! request, and `front`/`admin` waiting on its `depends_on`. The launcher
    //! is a strict fake with a fake clock and fake probes that records every
    //! spawn; no process, listener or runtime is involved.

    use std::cell::Cell;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use effigy_containers::{
        EffectiveContainerPolicy, EffectiveHostProcess, EffectiveManagedHostListener,
        HostProcessRestart, HostProcessSignal,
    };
    use effigy_tui::waits::{Attribution, EntryPhase, EntryState, WaitOn};

    use super::*;

    pub(super) const ENVIRONMENT: &str = "web";

    /// A clock the test advances: the same `Instant` values production reads.
    pub(super) struct Clock(Cell<Instant>);

    impl Clock {
        pub(super) fn new() -> Self {
            Self(Cell::new(Instant::now()))
        }

        pub(super) fn at(&self) -> Instant {
            self.0.get()
        }

        pub(super) fn advance(&self, millis: u64) {
            self.0.set(self.0.get() + Duration::from_millis(millis));
        }
    }

    fn listener() -> EffectiveManagedHostListener {
        EffectiveManagedHostListener {
            bind: "127.0.0.1:0".to_owned(),
            readiness_path: "/".to_owned(),
            readiness_status: 200,
            readiness_timeout_secs: 60,
            declared_route_domain: "api.test".to_owned(),
            route_domain: "api.test".to_owned(),
            route_tls: false,
        }
    }

    fn process(name: &str, depends_on: &[&str], declares_listener: bool) -> EffectiveHostProcess {
        EffectiveHostProcess {
            name: name.to_owned(),
            run: format!("run-{name}"),
            cwd: PathBuf::new(),
            env: Vec::new(),
            depends_on: depends_on.iter().map(|name| (*name).to_owned()).collect(),
            listener: declares_listener.then(listener),
            restart: HostProcessRestart::Never,
            restart_delay_ms: 0,
            shutdown_signal: HostProcessSignal::Sigterm,
            shutdown_grace_secs: 1,
        }
    }

    /// The environment, `api` (with its declared readiness request), and the
    /// two entries that depend on it.
    pub(super) fn four_entry_policy(checkout: &Path) -> EffectiveContainerPolicy {
        let mut policy = crate::runner::test_support::effective_container_policy(
            ENVIRONMENT,
            "fixture-project",
            "app",
            checkout.join("compose.yml"),
        );
        policy.repo_root = checkout.to_path_buf();
        policy.profile = "effigy".to_owned();
        policy.host_processes = vec![
            process("api", &[], true),
            process("front", &["api"], false),
            process("admin", &["api"], false),
        ];
        policy
    }

    fn fixture() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("fixture directory");
        let checkout = dir.path().canonicalize().expect("canonical fixture");
        // A new session never reads the previous session's waits.
        WaitLedger::environment(&checkout, ENVIRONMENT).clear();
        (dir, checkout)
    }

    pub(super) fn records(checkout: &Path) -> Vec<EntryState> {
        WaitLedger::environment(checkout, ENVIRONMENT).read_all()
    }

    pub(super) fn state_of<'a>(states: &'a [EntryState], entry: &str) -> &'a EntryState {
        states
            .iter()
            .find(|state| state.entry == entry)
            .unwrap_or_else(|| panic!("{entry} publishes a record"))
    }

    /// A strict launcher: it records every spawn, answers readiness from the
    /// script, and publishes through the production progress callback.
    pub(super) struct FakeLauncher<'a> {
        pub(super) ledger: &'a WaitLedger,
        pub(super) clock: &'a Clock,
        /// Readiness outcomes, by process, consumed in launch order.
        pub(super) failure: Option<RunnerError>,
        pub(super) spawns: Vec<String>,
        /// The published records at each observed moment.
        pub(super) frames: Vec<Vec<EntryState>>,
    }

    impl FakeLauncher<'_> {
        fn observe(&mut self) {
            self.frames.push(self.ledger.read_all());
        }
    }

    impl HostLauncher for FakeLauncher<'_> {
        fn dependency_ready(
            &mut self,
            _dependency: &str,
            on_wait: &mut dyn FnMut(Instant),
        ) -> Result<(), RunnerError> {
            self.observe();
            // A reader's readiness is observed after a short wait.
            self.clock.advance(1_000);
            on_wait(self.clock.at());
            self.observe();
            Ok(())
        }

        fn launch(
            &mut self,
            process: &str,
            on_wait: &mut dyn FnMut(Instant),
        ) -> Result<Option<ManagedHostListenerResult>, RunnerError> {
            self.spawns.push(process.to_owned());
            self.observe();
            // The entry's own declared readiness request blocks for 12s.
            self.clock.advance(12_000);
            on_wait(self.clock.at());
            self.observe();
            if process == "api" {
                if let Some(error) = self.failure.take() {
                    return Err(error);
                }
            }
            Ok(None)
        }

        fn started(&self) -> &[(String, effigy_kernel::supervise::SupervisorClient)] {
            &[]
        }

        fn take_started(&mut self) -> Vec<(String, effigy_kernel::supervise::SupervisorClient)> {
            Vec::new()
        }
    }

    /// Frame by frame: the dependents name `api` while it starts, `api` names
    /// its own readiness request with elapsed seconds, and only then do the
    /// dependents start. Every frame is read from the published records — the
    /// same ones the tabs, the headless text and `status` read.
    #[test]
    fn the_four_entry_host_graph_publishes_each_entry_s_own_wait() {
        let (_dir, checkout) = fixture();
        let policy = four_entry_policy(&checkout);
        let ledger = WaitLedger::environment(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        let mut graph = HostWaitGraph::for_policy(
            &checkout,
            &policy,
            &BTreeSet::from(["api".to_owned(), "front".to_owned(), "admin".to_owned()]),
            clock.at(),
        );
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            failure: None,
            spawns: Vec::new(),
            frames: Vec::new(),
        };
        let outcome = run_host_plan(&ordered, None, &mut graph, &mut launcher, &|| clock.at())
            .expect("the plan finishes");
        assert!(outcome.listeners.is_empty());

        // Frame 0 is `api` launched with its declared readiness request still
        // unanswered: exactly the interval the operator was left guessing in.
        let api_starting = &launcher.frames[0];
        let api = state_of(api_starting, "api");
        assert_eq!(api.phase, EntryPhase::Starting);
        assert_eq!(
            api.waiting_on,
            Some(WaitOn::Probe {
                probe: "GET / -> 200".to_owned()
            })
        );
        assert_eq!(api.tab_label(), "api [starting: GET / -> 200 (0s)]");
        for dependent in ["front", "admin"] {
            let state = state_of(api_starting, dependent);
            assert_eq!(state.phase, EntryPhase::Waiting, "{dependent}");
            assert_eq!(
                state.waiting_on,
                Some(WaitOn::Entry {
                    name: "api".to_owned(),
                    phase: EntryPhase::Starting
                }),
                "{dependent} names api's phase while api is still becoming ready"
            );
            assert_eq!(
                state.headless_line().as_deref(),
                Some(format!("[managed] {dependent}: waiting for api (starting) (0s)").as_str())
            );
        }

        // Frame 1 is the same wait 12s later: elapsed time belongs to the wait
        // each entry holds, and the probe names its request and its seconds.
        let api_waiting = &launcher.frames[1];
        let api = state_of(api_waiting, "api");
        assert_eq!(api.elapsed_ms, 12_000);
        assert_eq!(api.status_line(), "waiting for GET / -> 200 (12s)");
        for dependent in ["front", "admin"] {
            let state = state_of(api_waiting, dependent);
            assert_eq!(state.elapsed_ms, 12_000, "{dependent}");
            assert_eq!(
                state.status_line(),
                "waiting for api (starting) (12s)",
                "{dependent}"
            );
        }

        // Both dependents started, in order, and every wait settled.
        assert_eq!(launcher.spawns, vec!["api", "front", "admin"]);
        let settled = records(&checkout);
        assert!(
            settled.iter().all(|state| !state.holds_a_wait()),
            "no settled wait lingers: {settled:?}"
        );
    }

    /// A definitive prerequisite failure is terminal and attributed, and no
    /// dependent spawn is recorded.
    #[test]
    fn a_failed_prerequisite_spawns_no_dependent_and_attributes_each_one() {
        let (_dir, checkout) = fixture();
        let policy = four_entry_policy(&checkout);
        let ledger = WaitLedger::environment(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        let mut graph = HostWaitGraph::for_policy(
            &checkout,
            &policy,
            &BTreeSet::from(["api".to_owned(), "front".to_owned(), "admin".to_owned()]),
            clock.at(),
        );
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            failure: Some(RunnerError::ManagedHostReadiness {
                process: "api".to_owned(),
                diagnostic: "readiness probe GET / returned 500; the listener never became ready"
                    .to_owned(),
            }),
            spawns: Vec::new(),
            frames: Vec::new(),
        };
        let error = run_host_plan(&ordered, None, &mut graph, &mut launcher, &|| clock.at())
            .expect_err("api never became ready");
        assert!(matches!(error, RunnerError::ManagedHostReadiness { .. }));
        assert_eq!(
            launcher.spawns,
            vec!["api"],
            "no dependent spawn is recorded after its prerequisite failed"
        );

        let published = records(&checkout);
        let api = state_of(&published, "api");
        assert_eq!(api.phase, EntryPhase::Failed);
        assert!(
            api.status_line().contains("500"),
            "api's own report names the status: {}",
            api.status_line()
        );
        for dependent in ["front", "admin"] {
            let state = state_of(&published, dependent);
            assert_eq!(state.phase, EntryPhase::NotStarted, "{dependent}");
            assert_eq!(
                state.not_started_because,
                Some(Attribution::new(
                    "api",
                    "failed: readiness probe GET / returned 500; the listener never became ready"
                )),
                "{dependent} names the prerequisite and its report"
            );
            assert_eq!(
                state.headless_line().as_deref(),
                Some(
                    format!(
                        "[managed] {dependent}: not started — api failed: readiness probe GET / \
                         returned 500; the listener never became ready"
                    )
                    .as_str()
                )
            );
        }
    }
}

#[cfg(test)]
mod environment_tests {
    //! The environment frame, driven by the production handoff: the run's
    //! owner publishes the host graph and its environment identity before the
    //! phase can block, `status` and the headless text read those same records,
    //! and the plan then starts the hosts only after the environment is ready.

    use std::time::Duration;

    use effigy_tui::waits::{EntryPhase, WaitOn};

    use super::tests::{four_entry_policy, records, state_of, Clock, FakeLauncher, ENVIRONMENT};
    use super::*;
    use crate::runner::managed_run::wait_views;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().expect("fixture directory");
        let checkout = dir.path().canonicalize().expect("canonical fixture");
        WaitLedger::environment(&checkout, ENVIRONMENT).clear();
        WaitLedger::task(&checkout, "dev", "default").clear();
        (dir, checkout)
    }

    #[test]
    fn the_run_publishes_every_host_wait_before_the_phase_can_block() {
        let (_dir, checkout) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        // The run's owner names its environment in its own scope first, so a
        // status read during bring-up finds the environment's records.
        WaitLedger::task(&checkout, "dev", "default").publish_environment_identity(ENVIRONMENT);
        let mut graph = HostWaitGraph::for_environment_phase(&checkout, &policy, clock.at());
        graph.environment_progress(3, 4, clock.at() + Duration::from_secs(2));

        let published = records(&checkout);
        for entry in ["api", "front", "admin"] {
            let state = state_of(&published, entry);
            assert_eq!(state.phase, EntryPhase::Waiting, "{entry}");
            assert_eq!(
                state.waiting_on,
                Some(WaitOn::Environment {
                    healthy: Some(3),
                    total: Some(4)
                }),
                "{entry} names the environment it waits for"
            );
            assert_eq!(state.tab_label(), format!("{entry} [waiting: environment]"));
            assert_eq!(
                state.headless_line().as_deref(),
                Some(
                    format!(
                        "[managed] {entry}: waiting for environment (containers 3/4 healthy) (0s)"
                    )
                    .as_str()
                )
            );
        }

        // `status` reads the run's own scope for the environment identity and
        // reports the same host waits while the phase is still running.
        let views = wait_views(&checkout, "dev", "default", Some(ENVIRONMENT));
        assert_eq!(
            views
                .iter()
                .map(|view| (view.scope, view.state.entry.as_str(), view.state.phase))
                .collect::<Vec<_>>(),
            vec![
                ("host", "admin", EntryPhase::Waiting),
                ("host", "api", EntryPhase::Waiting),
                ("host", "front", EntryPhase::Waiting),
            ]
        );
        let json = serde_json::to_value(&views[1]).expect("view json");
        assert_eq!(json["scope"], "host");
        assert_eq!(json["waiting_on"]["kind"], "environment");
        assert_eq!(json["waiting_on"]["healthy"], 3);
        assert_eq!(json["waiting_on"]["total"], 4);

        // The phase reports ready, and only then does the plan start the hosts.
        graph.environment_ready(clock.at() + Duration::from_secs(5));
        let ledger = WaitLedger::environment(&checkout, ENVIRONMENT);
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            failure: None,
            spawns: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        run_host_plan(&ordered, None, &mut graph, &mut launcher, &|| clock.at())
            .expect("the plan finishes once the environment is ready");
        assert_eq!(launcher.spawns, vec!["api", "front", "admin"]);
        assert!(
            records(&checkout).iter().all(|state| !state.holds_a_wait()),
            "every wait settled"
        );
        assert!(
            wait_views(&checkout, "dev", "default", Some(ENVIRONMENT)).is_empty(),
            "a settled environment reports no waits"
        );
    }

    /// The environment's own failure is terminal for every host entry: none
    /// starts, and each names the environment and what it reported.
    #[test]
    fn an_environment_failure_marks_every_host_not_started() {
        let (_dir, checkout) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let mut graph = HostWaitGraph::for_environment_phase(&checkout, &policy, clock.at());
        graph.environment_failed(
            "`up` exited 1 after 4s; web is unhealthy after 3 failed checks",
            clock.at() + Duration::from_secs(4),
        );
        let published = records(&checkout);
        for entry in ["api", "front", "admin"] {
            let state = state_of(&published, entry);
            assert_eq!(state.phase, EntryPhase::NotStarted, "{entry}");
            assert_eq!(
                state
                    .not_started_because
                    .as_ref()
                    .map(|cause| cause.name.as_str()),
                Some("environment"),
                "{entry}"
            );
            assert!(
                state
                    .headless_line()
                    .unwrap_or_default()
                    .contains("unhealthy"),
                "{entry}: {:?}",
                state.headless_line()
            );
        }
        let ledger = WaitLedger::environment(&checkout, ENVIRONMENT);
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            failure: None,
            spawns: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        let error = run_host_plan(&ordered, None, &mut graph, &mut launcher, &|| clock.at())
            .expect_err("no host process starts after its environment failed");
        assert!(error.to_string().contains("not started"), "{error}");
        assert!(
            launcher.spawns.is_empty(),
            "no host process spawned: {:?}",
            launcher.spawns
        );
    }
}
