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
use effigy_core::managed_waits::{should_publish, EntryState, EntryWaits, HostOrder, SessionOrder};

use std::collections::BTreeMap;

use super::EffectiveManagedHostListener;
use crate::runner::error::RunnerError;
use crate::runner::managed_wait::{WaitLedger, WaitPublisher};

/// The first dependency this activation already withheld, if this process
/// declared one.
fn withheld_dependency(
    dependencies: &[String],
    withheld: &BTreeMap<String, (String, Option<u16>)>,
) -> Option<(String, String, Option<u16>)> {
    dependencies.iter().find_map(|dependency| {
        withheld
            .get(dependency.as_str())
            .map(|(detail, status)| (dependency.clone(), detail.clone(), *status))
    })
}

/// The attribution a withheld entry names: the process or prerequisite and the
/// first line of why its readiness was never verified, bounded so a diagnostic
/// never becomes a tab title (effigy#458).
pub(in crate::runner) fn withheld_attribution(
    holder: &str,
    reason: &str,
) -> effigy_core::managed_waits::Attribution {
    let line = reason
        .lines()
        .next()
        .unwrap_or_default()
        .trim()
        .trim_end_matches('.')
        .chars()
        .take(160)
        .collect::<String>();
    effigy_core::managed_waits::Attribution::new(
        holder,
        if line.is_empty() {
            "did not become ready".to_owned()
        } else {
            format!("did not become ready: {line}")
        },
    )
}

/// The run's environment-failure reason, kept together with the failure the
/// host planner published when it owned the scope. The planner is the only
/// process that saw which entry failed and what it reported, so its attribution
/// is the one a person needs: `...; api failed: readiness probe GET / returned
/// 500` (effigy#458).
pub(in crate::runner) fn attributed_environment_failure(
    reason: &str,
    ledger: &WaitLedger,
) -> String {
    let published = ledger.read_all();
    let failed = published
        .iter()
        .find(|state| state.phase == effigy_core::managed_waits::EntryPhase::Failed)
        .and_then(|state| {
            state
                .detail
                .as_deref()
                .map(|detail| format!("{} {detail}", state.entry))
        })
        .or_else(|| {
            published.iter().find_map(|state| {
                state
                    .not_started_because
                    .as_ref()
                    .map(|cause| format!("{} not started — {}", state.entry, cause.text()))
            })
        });
    match failed {
        Some(failure) => format!("{reason}; {failure}"),
        None => reason.to_owned(),
    }
}

/// The pure host ordering model and its complete run-owned snapshot.
pub(in crate::runner) struct HostWaitGraph {
    publisher: WaitPublisher,
    waits: EntryWaits,
    /// A restart publishes only entries it replaces.
    owned: BTreeSet<String>,
    published: Vec<EntryState>,
    published_at: Option<Instant>,
}

impl HostWaitGraph {
    /// Publish prelaunch holds on the managed parent's own run before the
    /// environment can block. Its matching activation run later supplies the
    /// host snapshot without sharing a writer or replacing an owner pointer.
    pub(in crate::runner) fn for_environment_phase(
        policy: &EffectiveContainerPolicy,
        publisher: &WaitPublisher,
        now: Instant,
    ) -> Self {
        publisher.settle_phase();
        let started = policy
            .host_processes
            .iter()
            .map(|process| process.name.clone())
            .collect();
        Self::new(policy, &started, publisher, Some(&policy.name), now)
    }

    /// A selected dependent still waits for the reader this activation
    /// reconfirms; unselected processes are running but never claimed ready.
    pub(in crate::runner) fn for_policy(
        policy: &EffectiveContainerPolicy,
        started: &BTreeSet<String>,
        publisher: &WaitPublisher,
        now: Instant,
    ) -> Self {
        Self::new(policy, started, publisher, None, now)
    }

    fn new(
        policy: &EffectiveContainerPolicy,
        started: &BTreeSet<String>,
        publisher: &WaitPublisher,
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
                waits.running(&process.name, now);
            }
        }
        let mut graph = Self {
            publisher: publisher.clone(),
            waits,
            owned: started.clone(),
            published: Vec::new(),
            published_at: None,
        };
        graph.publish_all(now);
        graph
    }

    /// Publish a complete snapshot when its meaning changes or a live wait
    /// refreshes. One journal event never exposes a partial set of entries.
    pub(in crate::runner) fn publish_all(&mut self, now: Instant) {
        let states = self.owned_states(now);
        if self.published_at.is_some()
            && !states.iter().any(|state| {
                should_publish(
                    self.published
                        .iter()
                        .find(|previous| previous.entry == state.entry),
                    self.published_at,
                    state,
                    now,
                )
            })
        {
            return;
        }
        self.publisher.publish_hosts(&states);
        self.published = states;
        self.published_at = Some(now);
    }

    fn owned_states(&mut self, now: Instant) -> Vec<EntryState> {
        self.waits.refresh(now);
        self.waits
            .states(now)
            .into_iter()
            .filter(|state| self.owned.contains(&state.entry))
            .collect()
    }

    /// The readiness owner reports its phase and this graph's corresponding
    /// host waits in one observation. A reset Ready phase carries no readiness
    /// proof; only the lifecycle's successful readiness result releases hosts.
    pub(in crate::runner) fn observe_environment_phase(
        &mut self,
        phase: &EntryState,
        now: Instant,
    ) {
        use effigy_core::managed_waits::{EntryPhase, WaitOn};
        if let Some(WaitOn::Environment {
            healthy: Some(healthy),
            total: Some(total),
        }) = &phase.waiting_on
        {
            self.waits.environment_progress(*healthy, *total);
        }
        if phase.phase == EntryPhase::Failed {
            self.waits.environment_failed(
                phase
                    .detail
                    .as_deref()
                    .unwrap_or("environment readiness failed"),
                now,
            );
        }
        let states = self.owned_states(now);
        self.publisher.publish_environment(phase, &states);
        self.published = states;
        self.published_at = Some(now);
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

    /// This run's environment failed; a matching child activation's attributed
    /// result is independently selected by readers and cannot be overwritten.
    pub(in crate::runner) fn environment_failed(&mut self, detail: &str, now: Instant) {
        self.waits.environment_failed(detail, now);
        self.publish_all(now);
    }

    /// The environment reported ready: the host entries now wait on their own
    /// `depends_on` readers instead of the phase.
    pub(in crate::runner) fn environment_ready(&mut self, now: Instant) {
        self.waits.environment_ready(now);
        self.publish_all(now);
    }

    /// Settle this run's successful entries, preserving failures and attributed
    /// non-starts. The whole terminal snapshot is published atomically.
    pub(in crate::runner) fn settle_all(&mut self, now: Instant) {
        for state in self.waits.states(now) {
            if self.owned.contains(&state.entry)
                && !matches!(
                    state.phase,
                    effigy_core::managed_waits::EntryPhase::NotStarted
                        | effigy_core::managed_waits::EntryPhase::Failed
                )
            {
                self.waits.ready(&state.entry, now);
            }
        }
        self.publish_all(now);
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

    /// A launched listener whose own readiness was withheld: it started and is
    /// terminal as a failure, and its dependents are attributed to that failure
    /// rather than to a start that never happened (effigy#458).
    pub(in crate::runner) fn listener_failed(&mut self, entry: &str, reason: &str, now: Instant) {
        self.waits.listener_failed(entry, reason, now);
        self.publish_all(now);
    }

    /// The plan withheld this entry — its own readiness, or a prerequisite's,
    /// never became verified. Only this entry is terminal: the plan still
    /// starts every entry that does not need it (effigy#458).
    pub(in crate::runner) fn withheld(
        &mut self,
        entry: &str,
        attribution: effigy_core::managed_waits::Attribution,
        now: Instant,
    ) {
        self.waits.withheld(entry, attribution, now);
        self.publish_all(now);
    }

    /// The plan aborted before it reached every entry: each one it never
    /// started is terminal and names what ended the plan (effigy#458).
    pub(in crate::runner) fn abandon_plan(&mut self, detail: &str, now: Instant) {
        self.waits.abandon(
            effigy_core::managed_waits::Attribution::new("plan", detail),
            now,
        );
        self.publish_all(now);
    }
}

/// How one readiness observation ended, as the activation's ordering loop
/// needs it: verified, or withheld with the reason and observed status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum ReadinessOutcome {
    Ready,
    Withheld {
        diagnostic: String,
        observed_status: Option<u16>,
        /// The enclosing activation's observation budget ended the wait, so
        /// nothing further is started.
        budget_expired: bool,
    },
}

/// What the activation's ordering loop asks of whoever runs it. The kernel
/// starts real supervisors and waits for real listeners; a strict fake records
/// spawns and answers readiness from a script. The loop below is then the one
/// production scheduler, so a fake proof exercises the real spawn and
/// transition decisions (effigy#458).
pub(in crate::runner) trait HostActivation {
    /// The listener this process declares, resolved for this activation.
    fn listener(&self, process: &EffectiveHostProcess) -> Option<EffectiveManagedHostListener>;

    /// Waits for `dependency`'s verified readiness, reporting progress so the
    /// wait it holds is published while it blocks.
    fn dependency_ready(
        &mut self,
        dependency: &str,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<ReadinessOutcome, RunnerError>;

    /// Forgets the dependency records collected for an earlier entry: a
    /// withheld entry's verified readers must never enter the next entry's
    /// runtime specification.
    fn discard_dependencies(&mut self);

    /// The replace step: settle the previous generation of this exact process
    /// and start its successor. Its specification carries only the records
    /// collected for this process.
    fn launch(&mut self, process: &EffectiveHostProcess) -> Result<(), RunnerError>;

    /// Waits for the launched listener's own declared readiness. A withheld
    /// result has already settled the generation this activation started.
    fn await_listener(
        &mut self,
        process: &EffectiveHostProcess,
        on_wait: &mut dyn FnMut(Instant),
    ) -> Result<ReadinessOutcome, RunnerError>;

    /// Reconciles a listener a previous activation published, once this
    /// activation's observation budget is spent.
    fn reconcile(
        &mut self,
        process: &EffectiveHostProcess,
    ) -> Result<ReadinessOutcome, RunnerError>;

    /// Withdraws the listener of a process this activation will not start,
    /// settling any generation it ran. Returns the detail it recorded.
    fn withdraw(
        &mut self,
        process: &EffectiveHostProcess,
        detail: String,
        observed_status: Option<u16>,
    ) -> Result<String, RunnerError>;
}

/// The activation's ordering loop: the one place that decides what starts, what
/// is withheld and what every entry's wait says. In dependency order it waits
/// for each declared reader, launches the process, observes its own declared
/// readiness and publishes the transition; a withheld entry is terminal and
/// attributed while everything that does not need it still starts. Once the
/// activation's observation budget is spent nothing further starts: a dependent
/// of a withheld entry is withheld, and a listener a previous activation
/// published is reconciled rather than restarted.
pub(in crate::runner) fn run_host_activation(
    ordered: &[&EffectiveHostProcess],
    selection: Option<&BTreeSet<String>>,
    graph: &mut HostWaitGraph,
    kernel: &mut dyn HostActivation,
    now: &dyn Fn() -> Instant,
) -> Result<(), RunnerError> {
    graph.publish_all(now());
    // What this activation withheld, so a dependent is withheld without waiting
    // on a record its prerequisite never published (effigy#455, #458).
    let mut withheld: BTreeMap<String, (String, Option<u16>)> = BTreeMap::new();
    let mut activation_budget_spent = false;
    for process in ordered {
        // `up` starts every declared process; a restart names the ones it
        // replaces and leaves the rest running.
        if selection.is_some_and(|selection| !selection.contains(&process.name)) {
            continue;
        }
        // One entry's records never cross into the next: a dependency verified
        // for a host this activation then withheld stays out of every later
        // specification.
        kernel.discard_dependencies();
        let listener = kernel.listener(process);
        // The activation's observation budget is spent: nothing further is
        // started. A dependency this activation withheld withholds this
        // process, and only then is a listener the previous activation
        // published reconciled against its own generation and route.
        if activation_budget_spent {
            if let Some((dependency, detail, observed_status)) =
                withheld_dependency(&process.depends_on, &withheld)
            {
                let detail = kernel.withdraw(
                    process,
                    format!(
                        "its dependency `{dependency}` was withheld by this activation: {detail}"
                    ),
                    observed_status,
                )?;
                graph.withheld(
                    &process.name,
                    withheld_attribution(&dependency, &detail),
                    now(),
                );
                withheld.insert(process.name.clone(), (detail, observed_status));
                continue;
            }
            if listener.is_none() {
                graph.withheld(
                    &process.name,
                    withheld_attribution(
                        &process.name,
                        "the activation's observation budget expired before this process was reached",
                    ),
                    now(),
                );
                continue;
            }
            match kernel.reconcile(process)? {
                ReadinessOutcome::Ready => {
                    graph.ready(&process.name, now());
                }
                ReadinessOutcome::Withheld {
                    diagnostic,
                    observed_status,
                    ..
                } => {
                    graph.withheld(
                        &process.name,
                        withheld_attribution(&process.name, &diagnostic),
                        now(),
                    );
                    withheld.insert(process.name.clone(), (diagnostic, observed_status));
                }
            }
            continue;
        }
        let mut dependency_refusal = None;
        for dependency in &process.depends_on {
            if let Some((diagnostic, observed_status)) = withheld.get(dependency.as_str()) {
                dependency_refusal = Some((
                    dependency.clone(),
                    format!("its dependency was withheld by this activation: {diagnostic}"),
                    *observed_status,
                ));
                break;
            }
            // Publish the wait this entry holds before blocking on the reader's
            // readiness, so a restart of a dependent names the reader it
            // re-confirms even when another activation started it.
            graph.publish_all(now());
            match kernel.dependency_ready(dependency, &mut |at| graph.publish_all(at))? {
                ReadinessOutcome::Ready => {
                    graph.dependency_ready(dependency, now());
                }
                ReadinessOutcome::Withheld {
                    diagnostic,
                    observed_status,
                    budget_expired,
                } => {
                    activation_budget_spent = budget_expired;
                    dependency_refusal = Some((dependency.clone(), diagnostic, observed_status));
                    break;
                }
            }
        }
        if let Some((dependency, diagnostic, observed_status)) = dependency_refusal {
            let detail = kernel.withdraw(
                process,
                format!("its dependency `{dependency}` did not become ready: {diagnostic}"),
                observed_status,
            )?;
            graph.withheld(
                &process.name,
                withheld_attribution(&dependency, &detail),
                now(),
            );
            withheld.insert(process.name.clone(), (detail, observed_status));
            continue;
        }
        kernel.launch(process)?;
        graph.started(&process.name, now());
        let Some(listener) = listener else {
            // A non-listener's launch is complete when it returns: the next
            // entry's hold is the next launch, not this finished one.
            graph.ready(&process.name, now());
            continue;
        };
        graph.probing(
            &process.name,
            effigy_core::managed_waits::listener_probe_label(
                &listener.readiness_path,
                listener.readiness_status,
            ),
            now(),
        );
        match kernel.await_listener(process, &mut |at| graph.publish_all(at))? {
            ReadinessOutcome::Ready => {
                graph.ready(&process.name, now());
            }
            ReadinessOutcome::Withheld {
                diagnostic,
                observed_status,
                budget_expired,
            } => {
                activation_budget_spent = budget_expired;
                graph.listener_failed(&process.name, &diagnostic, now());
                withheld.insert(
                    process.name.clone(),
                    (format!("failed: {diagnostic}"), observed_status),
                );
            }
        }
    }
    graph.settle_all(now());
    Ok(())
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
    use effigy_core::managed_waits::{Attribution, EntryPhase, EntryState, WaitOn};

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

    pub(super) fn process(
        name: &str,
        depends_on: &[&str],
        declares_listener: bool,
    ) -> EffectiveHostProcess {
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

    /// The four-entry graph plus one host that declares no dependency of its
    /// own: it is still held by the plan launching the entries before it.
    pub(super) fn policy_with_an_independent_host(checkout: &Path) -> EffectiveContainerPolicy {
        let mut policy = four_entry_policy(checkout);
        policy.host_processes.push(process("jobs", &[], false));
        policy
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, WaitPublisher) {
        let dir = tempfile::tempdir().expect("fixture directory");
        let checkout = dir.path().canonicalize().expect("canonical fixture");
        let publisher = WaitPublisher::fixture(&checkout, Some(ENVIRONMENT));
        (dir, checkout, publisher)
    }

    pub(super) fn records(checkout: &Path, publisher: &WaitPublisher) -> Vec<EntryState> {
        publisher
            .environment_source(checkout, ENVIRONMENT)
            .read_all()
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
        /// The process whose own readiness the script withholds, and why.
        pub(super) withheld: Option<(String, String)>,
        pub(super) spawns: Vec<String>,
        /// The entries whose listener this activation withdrew.
        pub(super) withdrawn: Vec<String>,
        /// The dependencies collected for the process about to launch.
        pub(super) pending: Vec<String>,
        /// What each launch's specification carried, in launch order.
        pub(super) spec_dependencies: Vec<Vec<String>>,
        /// The published records at each observed moment.
        pub(super) frames: Vec<Vec<EntryState>>,
    }

    impl FakeLauncher<'_> {
        fn observe(&mut self) {
            self.frames.push(self.ledger.read_all());
        }
    }

    impl HostActivation for FakeLauncher<'_> {
        fn listener(&self, process: &EffectiveHostProcess) -> Option<EffectiveManagedHostListener> {
            process.listener.clone()
        }

        fn dependency_ready(
            &mut self,
            dependency: &str,
            on_wait: &mut dyn FnMut(Instant),
        ) -> Result<ReadinessOutcome, RunnerError> {
            self.observe();
            let withheld = self
                .withheld
                .as_ref()
                .filter(|(name, _)| name == dependency)
                .map(|(_, reason)| reason.clone());
            if let Some(reason) = withheld {
                // A prerequisite that failed its own readiness withholds every
                // dependent exactly as the kernel reports it.
                return Ok(ReadinessOutcome::Withheld {
                    diagnostic: format!("failed: {reason}"),
                    observed_status: Some(500),
                    budget_expired: false,
                });
            }
            // A reader's readiness is observed after a short wait.
            self.clock.advance(1_000);
            on_wait(self.clock.at());
            self.observe();
            self.pending.push(dependency.to_owned());
            Ok(ReadinessOutcome::Ready)
        }

        fn discard_dependencies(&mut self) {
            self.pending.clear();
        }

        fn launch(&mut self, process: &EffectiveHostProcess) -> Result<(), RunnerError> {
            self.spawns.push(process.name.clone());
            self.spec_dependencies
                .push(std::mem::take(&mut self.pending));
            Ok(())
        }

        fn await_listener(
            &mut self,
            process: &EffectiveHostProcess,
            on_wait: &mut dyn FnMut(Instant),
        ) -> Result<ReadinessOutcome, RunnerError> {
            let withheld = self
                .withheld
                .as_ref()
                .filter(|(name, _)| name == &process.name)
                .map(|(_, reason)| reason.clone());
            if let Some(reason) = withheld {
                // The launched listener answered its probe with a failure: it
                // started, and its own readiness never verified.
                self.observe();
                return Ok(ReadinessOutcome::Withheld {
                    diagnostic: reason,
                    observed_status: Some(500),
                    budget_expired: false,
                });
            }
            self.observe();
            // The entry's own declared readiness request blocks for 12s.
            self.clock.advance(12_000);
            on_wait(self.clock.at());
            self.observe();
            Ok(ReadinessOutcome::Ready)
        }

        fn reconcile(
            &mut self,
            _process: &EffectiveHostProcess,
        ) -> Result<ReadinessOutcome, RunnerError> {
            // This script never spends the activation's observation budget.
            Ok(ReadinessOutcome::Ready)
        }

        fn withdraw(
            &mut self,
            process: &EffectiveHostProcess,
            detail: String,
            _observed_status: Option<u16>,
        ) -> Result<String, RunnerError> {
            self.withdrawn.push(process.name.clone());
            Ok(detail)
        }
    }

    /// The production ordering loop, driven by the strict fake.
    pub(super) fn run_plan(
        ordered: &[&EffectiveHostProcess],
        selection: Option<&BTreeSet<String>>,
        graph: &mut HostWaitGraph,
        launcher: &mut FakeLauncher<'_>,
        clock: &Clock,
    ) -> Result<(), RunnerError> {
        run_host_activation(ordered, selection, graph, launcher, &|| clock.at())
    }

    /// Frame by frame: the dependents name `api` while it starts, `api` names
    /// its own readiness request with elapsed seconds, and only then do the
    /// dependents start. Every frame is read from the published records — the
    /// same ones the tabs, the headless text and `status` read.
    #[test]
    fn the_four_entry_host_graph_publishes_each_entry_s_own_wait() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        let mut graph = HostWaitGraph::for_policy(
            &policy,
            &BTreeSet::from(["api".to_owned(), "front".to_owned(), "admin".to_owned()]),
            &publisher,
            clock.at(),
        );
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock).expect("the plan finishes");

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
        let settled = records(&checkout, &publisher);
        assert!(
            settled.iter().all(|state| !state.holds_a_wait()),
            "no settled wait lingers: {settled:?}"
        );
    }

    /// A definitive prerequisite failure is terminal and attributed, and no
    /// dependent spawn is recorded.
    #[test]
    fn a_withheld_prerequisite_settles_each_dependent_without_spawning_it() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        let mut graph = HostWaitGraph::for_policy(
            &policy,
            &BTreeSet::from(["api".to_owned(), "front".to_owned(), "admin".to_owned()]),
            &publisher,
            clock.at(),
        );
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: Some((
                "api".to_owned(),
                "readiness probe GET / returned 500; the listener never became ready".to_owned(),
            )),
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock)
            .expect("a withheld listener does not abort the plan");
        assert_eq!(
            launcher.spawns,
            vec!["api"],
            "no dependent spawn is recorded after its prerequisite was withheld"
        );

        let published = records(&checkout, &publisher);
        let api = state_of(&published, "api");
        assert_eq!(
            api.phase,
            EntryPhase::Failed,
            "the launched listener failed, it did not fail to start"
        );
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
                        "[managed] {dependent}: not started — api failed: readiness \
                         probe GET / returned 500; the listener never became ready"
                    )
                    .as_str()
                )
            );
        }
    }
}

#[cfg(test)]
mod environment_tests {
    //! The environment frame from actual parent/activation run snapshots:
    //! prelaunch holds, readiness counts and host transitions are the same
    //! observations in status, headless text and panes.

    use std::time::Duration;

    use effigy_core::managed_waits::{EntryPhase, WaitOn};

    use super::tests::{
        four_entry_policy, policy_with_an_independent_host, process, records, run_plan, state_of,
        Clock, FakeLauncher, ENVIRONMENT,
    };
    use super::*;
    use crate::runner::managed_run::wait_views;

    fn fixture() -> (tempfile::TempDir, std::path::PathBuf, WaitPublisher) {
        let dir = tempfile::tempdir().expect("fixture directory");
        let checkout = dir.path().canonicalize().expect("canonical fixture");
        let publisher = WaitPublisher::fixture(&checkout, Some(ENVIRONMENT));
        (dir, checkout, publisher)
    }

    #[test]
    fn activation_readiness_publishes_phase_counts_host_clocks_and_failure_together() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let parent = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        let activation = WaitPublisher::fixture_child(&checkout, ENVIRONMENT, publisher.run_id());
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &activation, clock.at());
        drop(parent);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let mut phase = EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Ready,
            waiting_on: None,
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        };
        graph.observe_environment_phase(&phase, clock.at());
        assert_eq!(ledger.read_phase(), Some(phase.clone()));
        assert_eq!(
            ledger
                .read_all()
                .iter()
                .map(|state| state.entry.as_str())
                .collect::<Vec<_>>(),
            vec!["admin", "api", "front"],
            "the activation snapshot contains every declared host"
        );
        assert!(
            ledger.read_all().iter().all(|state| state.waiting_on
                == Some(WaitOn::Environment {
                    healthy: None,
                    total: None
                })),
            "a count reset is not proof that the environment became ready"
        );
        phase.phase = EntryPhase::Waiting;
        phase.waiting_on = Some(WaitOn::Environment {
            healthy: Some(3),
            total: Some(4),
        });
        phase.elapsed_ms = 9_000;
        graph.observe_environment_phase(&phase, clock.at());
        assert_eq!(ledger.read_phase(), Some(phase.clone()));
        for state in ledger.read_all() {
            assert_eq!(
                state.waiting_on, phase.waiting_on,
                "{} carries the same 3/4 observation",
                state.entry
            );
            assert_eq!(
                state.elapsed_ms, 0,
                "{} starts its own current wait clock",
                state.entry
            );
        }
        clock.advance(2_000);
        phase.elapsed_ms = 11_000;
        graph.observe_environment_phase(&phase, clock.at());
        assert_eq!(ledger.read_phase(), Some(phase.clone()));
        assert!(
            ledger
                .read_all()
                .iter()
                .all(|state| state.elapsed_ms == 2_000),
            "child host clocks keep refreshing after the managed parent's graph is dropped"
        );
        phase.phase = EntryPhase::Failed;
        phase.waiting_on = None;
        phase.detail = Some("web is unhealthy after 3 failed checks".to_owned());
        graph.observe_environment_phase(&phase, clock.at());
        assert_eq!(ledger.read_phase(), Some(phase));
        for state in ledger.read_all() {
            assert_eq!(state.phase, EntryPhase::NotStarted);
            let cause = state
                .not_started_because
                .expect("the environment failure is attributed");
            assert_eq!(cause.name, "environment");
            assert!(cause.detail.contains("unhealthy"));
        }
    }

    #[test]
    fn the_run_publishes_every_host_wait_before_the_phase_can_block() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        // The run's owner names its environment in its own scope first, so a
        // status read during bring-up finds the environment's records.
        publisher.publish_environment_identity(ENVIRONMENT);
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        graph.environment_progress(3, 4, clock.at() + Duration::from_secs(2));

        let published = records(&checkout, &publisher);
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
        let views = wait_views(&checkout, publisher.run_id(), Some(ENVIRONMENT));
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
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = policy.host_processes.iter().collect::<Vec<_>>();
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock)
            .expect("the plan finishes once the environment is ready");
        assert_eq!(launcher.spawns, vec!["api", "front", "admin"]);
        assert!(
            records(&checkout, &publisher)
                .iter()
                .all(|state| !state.holds_a_wait()),
            "every wait settled"
        );
        assert!(
            wait_views(&checkout, publisher.run_id(), Some(ENVIRONMENT)).is_empty(),
            "a settled environment reports no waits"
        );
    }

    /// The frame a person would see for this session's published records.
    fn frame_of(tabs: &[&str], active: &str, published: &[EntryState]) -> String {
        let names = tabs
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();
        effigy_tui::multiprocess::render_snapshot_frame(effigy_tui::multiprocess::FrameSnapshot {
            process_names: &names,
            active,
            entry_waits: published,
            logs: &[],
            width: 200,
            height: 30,
        })
        .expect("the frame renders")
    }

    fn frame(
        checkout: &std::path::Path,
        publisher: &WaitPublisher,
        tabs: &[&str],
        active: &str,
    ) -> String {
        frame_of(tabs, active, &records(checkout, publisher))
    }

    fn follower(
        checkout: &std::path::Path,
        publisher: &WaitPublisher,
        entry: &str,
    ) -> crate::runner::managed_run::HostFollower {
        let ledger = publisher.environment_source(checkout, ENVIRONMENT);
        crate::runner::managed_run::HostFollower::new(crate::runner::managed_run::HostProcessTab {
            identity: None,
            name: entry.to_owned(),
            log: std::path::PathBuf::from(format!("/nonexistent/{entry}.log")),
            listener_state: None,
            restart: effigy_containers::HostProcessRestart::Never,
            restart_delay_ms: 0,
            wait_ledger: ledger,
        })
    }

    /// Real successive operations retain complete snapshots and exact parent
    /// lineage; no publisher pointer or staged-directory protocol remains.
    #[test]
    fn complete_activation_snapshots_preserve_parent_lineage_and_terminal_attribution() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let mut parent = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let activation = WaitPublisher::fixture_child(&checkout, ENVIRONMENT, publisher.run_id());
        let names = |states: &[EntryState]| {
            states
                .iter()
                .map(|state| state.entry.clone())
                .collect::<Vec<_>>()
        };
        let prelaunch = ledger.read_all();
        assert_eq!(names(&prelaunch), vec!["admin", "api", "front"]);
        assert!(prelaunch
            .iter()
            .all(|state| state.phase == EntryPhase::Waiting));
        assert!(
            !ledger.activation_started(),
            "unknown child hosts preserve the whole parent snapshot"
        );
        let mut tab = follower(&checkout, &publisher, "api");
        tab.poll();
        assert!(
            !tab.may_announce_started(),
            "an unobserved child cannot announce a start"
        );
        let all = policy
            .host_processes
            .iter()
            .map(|process| process.name.clone())
            .collect();
        let mut planner = HostWaitGraph::for_policy(&policy, &all, &activation, clock.at());
        assert!(ledger.activation_started());
        assert_eq!(
            names(&ledger.read_all()),
            names(&prelaunch),
            "the first host publication is complete"
        );
        planner.started("api", clock.at());
        planner.probing("api", "GET / -> 200".to_owned(), clock.at());
        for tick in 1..=4 {
            let now = clock.at() + Duration::from_millis(250 * tick);
            parent.environment_progress(3, 4, now);
            parent.environment_ready(now);
            parent.publish_all(now);
            assert_eq!(
                state_of(&ledger.read_all(), "api").waiting_on,
                Some(WaitOn::Probe {
                    probe: "GET / -> 200".to_owned()
                })
            );
        }
        planner.listener_failed("api", "readiness probe GET / returned 500", clock.at());
        let reason = "environment web did not come up: up exited 1 after 4s";
        parent.environment_failed(reason, clock.at());
        parent.settle_all(clock.at());
        let terminal = ledger.read_all();
        assert_eq!(state_of(&terminal, "api").phase, EntryPhase::Failed);
        assert!(state_of(&terminal, "api")
            .detail
            .as_deref()
            .unwrap()
            .contains("500"));
        for dependent in ["front", "admin"] {
            let state = state_of(&terminal, dependent);
            assert_eq!(state.phase, EntryPhase::NotStarted);
            let cause = state.not_started_because.as_ref().unwrap();
            assert_eq!(cause.name, "api");
            assert!(cause.detail.contains("500"));
        }
        let attributed = attributed_environment_failure(reason, &ledger);
        assert!(
            attributed.starts_with(reason)
                && attributed.contains("api")
                && attributed.contains("500")
        );
        drop(planner);
        drop(activation);
        assert_eq!(
            ledger.read_all(),
            terminal,
            "terminal failure detail survives owner settlement"
        );
        let foreign_parent = WaitPublisher::fixture(&checkout, Some(ENVIRONMENT));
        let foreign = WaitPublisher::fixture_child(&checkout, ENVIRONMENT, foreign_parent.run_id());
        foreign.publish_hosts(&[]);
        assert_eq!(
            ledger.read_all(),
            terminal,
            "a successive foreign-parent activation cannot replace this view"
        );
    }

    /// A host that declares no dependency of its own is still held while the
    /// plan launches entries one at a time. It names the launch the plan is
    /// performing now — the real scheduler position, with its actual phase, in
    /// the resolved order — not an invented dependency and not a queued
    /// neighbour (effigy#458).
    #[test]
    fn an_independent_host_names_the_schedulers_active_launch() {
        let (_dir, checkout, publisher) = fixture();
        let policy = policy_with_an_independent_host(&checkout);
        let clock = Clock::new();
        publisher.publish_environment_identity(ENVIRONMENT);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(4),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        let (healthy, total) = ledger
            .read_progress()
            .expect("the phase reports its containers");
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        graph.environment_progress(healthy, total, clock.at());
        graph.environment_ready(clock.at());

        // While `api` starts and becomes ready, the independent `jobs` names
        // the entry whose launch holds it.
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock).expect("the plan finishes");

        // While `api` probes, `jobs` names that active launch — several
        // positions ahead of it — not the queued `admin` behind it.
        let during_api = &launcher.frames[0];
        let jobs = state_of(during_api, "jobs");
        assert_eq!(jobs.phase, EntryPhase::Waiting);
        assert_eq!(
            jobs.waiting_on,
            Some(WaitOn::Launch {
                name: "api".to_owned(),
                phase: EntryPhase::Starting
            }),
            "the hold names the active launch in the resolved order"
        );
        assert_eq!(
            jobs.waiting_on.as_ref().and_then(WaitOn::dependency),
            None,
            "the scheduler hold is not a dependency"
        );
        let presented = frame_of(&["api", "front", "admin", "jobs"], "jobs", during_api);
        assert!(
            presented.contains("jobs [waiting: api starting]"),
            "{presented}"
        );
        assert!(
            presented.contains("waiting for api's launch (starting)"),
            "{presented}"
        );
        assert_eq!(
            jobs.headless_line().as_deref(),
            Some("[managed] jobs: waiting for api's launch (starting) (0s)")
        );
        // The same record carries the machine-readable identity and state; the
        // status projection reads exactly these records (the plan has finished
        // by now, so its own snapshot is settled).
        let json = serde_json::to_value(jobs).expect("record json");
        assert_eq!(json["phase"], "waiting");
        assert_eq!(json["waiting_on"]["kind"], "launch");
        assert_eq!(json["waiting_on"]["name"], "api");
        assert_eq!(json["waiting_on"]["phase"], "starting");
        assert_eq!(launcher.spawns, vec!["api", "front", "admin", "jobs"]);
    }

    /// A host with two dependencies is withheld when its second reader is:
    /// the first reader it verified is not left in the activation's collected
    /// records, so the next independent host's specification declares no
    /// dependency it never asked for (effigy#458).
    #[test]
    fn a_withheld_entry_takes_no_verified_dependency_into_the_next_spec() {
        let (_dir, checkout, publisher) = fixture();
        let mut policy = four_entry_policy(&checkout);
        policy.host_processes = vec![
            process("api", &[], true),
            process("auth", &[], true),
            process("front", &["api", "auth"], false),
            process("jobs", &[], false),
        ];
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        let all = policy
            .host_processes
            .iter()
            .map(|process| process.name.clone())
            .collect::<BTreeSet<_>>();
        let mut graph = HostWaitGraph::for_policy(&policy, &all, &publisher, clock.at());
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: Some((
                "auth".to_owned(),
                "readiness probe GET / returned 500; the listener never became ready".to_owned(),
            )),
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock)
            .expect("a withheld listener does not abort the plan");

        // `api` verified for the withheld `front`, and `front` never launched:
        // every launch carries its own dependencies only.
        assert_eq!(launcher.spawns, vec!["api", "auth", "jobs"]);
        assert_eq!(
            launcher.spec_dependencies,
            vec![Vec::<String>::new(), Vec::new(), Vec::new()],
            "no launch inherits a withheld entry's verified readers"
        );
        let published = records(&checkout, &publisher);
        let front = state_of(&published, "front");
        assert_eq!(front.phase, EntryPhase::NotStarted);
        assert_eq!(
            front
                .not_started_because
                .as_ref()
                .map(|cause| cause.name.as_str()),
            Some("auth"),
            "the withheld host is attributed to its second reader"
        );
        // And the independent host started with no dependency edge at all.
        assert_eq!(state_of(&published, "jobs").phase, EntryPhase::Ready);
    }

    /// A completed non-listener launch stops holding the scheduler: while the
    /// next entry blocks re-confirming a reader another activation started, an
    /// independent queued entry names nothing, and once that next entry's own
    /// launch is active it names that launch (effigy#458).
    #[test]
    fn a_completed_non_listener_launch_does_not_hold_the_scheduler() {
        let (_dir, checkout, publisher) = fixture();
        let mut policy = four_entry_policy(&checkout);
        // `api` runs outside this call; `seed` completes before `front` blocks.
        policy.host_processes = vec![
            process("api", &[], true),
            process("seed", &[], false),
            process("front", &["api"], true),
            process("jobs", &[], false),
        ];
        let selected = BTreeSet::from(["seed".to_owned(), "front".to_owned(), "jobs".to_owned()]);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        let clock = Clock::new();
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        let mut graph = HostWaitGraph::for_policy(&policy, &selected, &publisher, clock.at());
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        run_plan(&ordered, Some(&selected), &mut graph, &mut launcher, &clock)
            .expect("the plan finishes");

        // Frame 0 is `front` blocked re-confirming `api`, after `seed`'s launch
        // returned and before `front` launches: the scheduler holds nothing, so
        // the independent `jobs` never names `seed`'s finished launch.
        let after_seed = &launcher.frames[0];
        let jobs = state_of(after_seed, "jobs");
        assert_eq!(
            jobs.waiting_on, None,
            "a finished non-listener launch is not the scheduler's hold: {jobs:?}"
        );
        // Frame 2 is `front` launched with its own probe unanswered: now the
        // hold is that active launch, named for the queued entry.
        let during_front = &launcher.frames[2];
        assert_eq!(
            state_of(during_front, "jobs").waiting_on,
            Some(WaitOn::Launch {
                name: "front".to_owned(),
                phase: EntryPhase::Starting
            })
        );
        assert_eq!(
            launcher.spawns,
            vec!["seed", "front", "jobs"],
            "the selected entries started once each"
        );
    }

    /// A restart of one dependent re-confirms a reader another call started.
    /// While the plan blocks on that reader's readiness, the entry names it —
    /// its phase, its elapsed time and its machine-readable identity — instead
    /// of publishing nothing (effigy#458).
    #[test]
    fn a_restarted_host_names_the_running_reader_it_reconfirms() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        publisher.publish_environment_identity(ENVIRONMENT);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        // `front` is restarted; `api` and `admin` run outside this call.
        let selected = BTreeSet::from(["front".to_owned()]);
        let mut graph = HostWaitGraph::for_policy(&policy, &selected, &publisher, clock.at());

        // The staged snapshot already names the reader it must re-confirm.
        let staged = records(&checkout, &publisher);
        let front = state_of(&staged, "front");
        assert_eq!(
            front.waiting_on,
            Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Running
            })
        );
        assert_eq!(front.tab_label(), "front [waiting: api running]");
        assert_eq!(
            front.headless_line().as_deref(),
            Some("[managed] front: waiting for api (running) (0s)")
        );
        let json = serde_json::to_value(front).expect("record json");
        assert_eq!(json["waiting_on"]["kind"], "entry");
        assert_eq!(json["waiting_on"]["name"], "api");
        assert_eq!(json["waiting_on"]["phase"], "running");

        // The plan blocks on api's readiness and keeps the wait live.
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        run_plan(&ordered, Some(&selected), &mut graph, &mut launcher, &clock)
            .expect("the restart finishes");
        let during_block = &launcher.frames[1];
        let front = state_of(during_block, "front");
        assert_eq!(
            front.waiting_on,
            Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Running
            }),
            "the wait survives the block"
        );
        assert_eq!(
            front.status_line(),
            "waiting for api (running) (1s)",
            "its elapsed time is its own"
        );
        assert_eq!(
            launcher.spawns,
            vec!["front"],
            "only the restarted host launched"
        );
        let settled = records(&checkout, &publisher);
        assert!(
            settled.iter().all(|state| !state.holds_a_wait()),
            "the restart settles its own record"
        );
        assert_eq!(
            settled
                .iter()
                .map(|state| state.entry.as_str())
                .collect::<Vec<_>>(),
            vec!["front"],
            "a restart never claims another call's running processes ready"
        );
    }

    /// Declaration order is not launch order: a host declared before its
    /// reader still waits for the reader's readiness, and a host declared first
    /// is not held by anything. The scheduler hold names the active launch out
    /// of the resolved plan (effigy#458).
    #[test]
    fn the_hold_follows_the_resolved_launch_order_not_the_declaration_order() {
        let (_dir, checkout, publisher) = fixture();
        let mut policy = four_entry_policy(&checkout);
        // Declared before the reader it depends on, and with an independent
        // host last.
        policy.host_processes = vec![
            process("front", &["api"], false),
            process("api", &[], true),
            process("jobs", &[], false),
        ];
        let clock = Clock::new();
        publisher.publish_environment_identity(ENVIRONMENT);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(4),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        let (healthy, total) = ledger
            .read_progress()
            .expect("the phase reports its containers");
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        graph.environment_progress(healthy, total, clock.at());
        graph.environment_ready(clock.at());
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: None,
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        assert_eq!(
            ordered
                .iter()
                .map(|process| process.name.as_str())
                .collect::<Vec<_>>(),
            vec!["api", "front", "jobs"],
            "the plan launches the reader first"
        );
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock).expect("the plan finishes");

        // While api probes, `front` names its declared dependency and `jobs`
        // names the active launch; nothing names a queued entry.
        let during_api = &launcher.frames[0];
        assert_eq!(
            state_of(during_api, "front").waiting_on,
            Some(WaitOn::Entry {
                name: "api".to_owned(),
                phase: EntryPhase::Starting
            })
        );
        assert_eq!(
            state_of(during_api, "jobs").waiting_on,
            Some(WaitOn::Launch {
                name: "api".to_owned(),
                phase: EntryPhase::Starting
            })
        );
        assert_eq!(
            state_of(during_api, "front").tab_label(),
            "front [waiting: api starting]"
        );
        assert_eq!(
            state_of(during_api, "jobs").tab_label(),
            "jobs [waiting: api starting]"
        );
        assert_eq!(launcher.spawns, vec!["api", "front", "jobs"]);
    }

    /// When a launch aborts the plan, every entry it never started is terminal
    /// and names what ended the plan — including a host with no declared
    /// dependency, which must not be left waiting (effigy#458).
    #[test]
    fn a_withheld_dependency_settles_dependents_while_an_independent_entry_starts() {
        let (_dir, checkout, publisher) = fixture();
        let policy = policy_with_an_independent_host(&checkout);
        let clock = Clock::new();
        publisher.publish_environment_identity(ENVIRONMENT);
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(4),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        let (healthy, total) = ledger
            .read_progress()
            .expect("the phase reports its containers");
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        graph.environment_progress(healthy, total, clock.at());
        graph.environment_ready(clock.at());
        let mut launcher = FakeLauncher {
            ledger: &ledger,
            clock: &clock,
            withheld: Some((
                "api".to_owned(),
                "readiness probe GET / returned 500; the listener never became ready".to_owned(),
            )),
            spawns: Vec::new(),
            withdrawn: Vec::new(),
            pending: Vec::new(),
            spec_dependencies: Vec::new(),
            frames: Vec::new(),
        };
        let ordered = crate::runner::host_process::order_host_processes(&policy.host_processes)
            .expect("the plan order resolves");
        run_plan(&ordered, None, &mut graph, &mut launcher, &clock)
            .expect("a withheld listener does not abort the plan");
        assert_eq!(
            launcher.spawns,
            vec!["api", "jobs"],
            "the dependents were withheld and the independent host still started"
        );

        let published = records(&checkout, &publisher);
        for dependent in ["front", "admin"] {
            assert_eq!(
                state_of(&published, dependent).phase,
                EntryPhase::NotStarted,
                "{dependent}"
            );
            assert_eq!(
                state_of(&published, dependent)
                    .not_started_because
                    .as_ref()
                    .map(|cause| cause.name.as_str()),
                Some("api"),
                "{dependent} names the failing prerequisite"
            );
        }
        let jobs = state_of(&published, "jobs");
        assert_eq!(
            jobs.phase,
            EntryPhase::Ready,
            "the independent host started and settled"
        );
        assert_eq!(jobs.waiting_on, None, "it waits for nothing");
        assert!(jobs.not_started_because.is_none());
        let presented = frame_of(&["api", "front", "admin", "jobs"], "jobs", &published);
        assert!(
            presented.contains("front [not started: api failed]"),
            "{presented}"
        );
        assert!(
            presented.contains("jobs [ready]"),
            "the independent entry is not settled by another entry's withheld start: {presented}"
        );
        let views = wait_views(&checkout, publisher.run_id(), Some(ENVIRONMENT));
        assert!(
            views.iter().all(|view| view.state.entry != "jobs"),
            "a settled entry is not reported as waiting"
        );
        let front = views
            .iter()
            .find(|view| view.state.entry == "front")
            .expect("front is reported");
        let json = serde_json::to_value(front).expect("view json");
        assert_eq!(json["phase"], "not_started");
        assert_eq!(json["not_started_because"]["name"], "api");
        assert!(
            json["not_started_because"]["detail"]
                .as_str()
                .unwrap_or_default()
                .contains("500"),
            "{json}"
        );
    }

    /// A named environment that declares no host processes still resets its
    /// phase at the run's boundary: no host graph is staged for it, so the
    /// reset cannot come from one, and an interrupted prior run's counts must
    /// never reach a workload's wait (effigy#458).
    #[test]
    fn a_named_environment_without_host_processes_still_resets_its_phase() {
        let (_dir, checkout, publisher) = fixture();
        let mut policy = four_entry_policy(&checkout);
        policy.host_processes.clear();
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(ledger.read_progress(), Some((3, 4)));

        // The run's boundary for this named environment, with no host graph.
        publisher.settle_phase();
        assert_eq!(
            ledger.read_progress(),
            None,
            "an interrupted prior run's counts never reach this run"
        );
        assert!(
            !ledger
                .read_phase()
                .is_some_and(|phase| phase.holds_a_wait()),
            "the phase is settled, not waiting"
        );
        // The readiness publisher remains the only source of counts.
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(4),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(ledger.read_progress(), Some((4, 4)));
    }

    /// The new session's boundary resets the phase record *before* the run
    /// starts polling it, so an interrupted prior run's container counts are
    /// never projected into this run (effigy#458).
    #[test]
    fn a_new_run_never_projects_the_previous_phases_counts() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        // A prior run was interrupted with three of four containers healthy.
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(3),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(ledger.read_progress(), Some((3, 4)));

        let previous = publisher;
        let publisher = WaitPublisher::fixture(&checkout, Some(ENVIRONMENT));
        let ledger = publisher.environment_source(&checkout, ENVIRONMENT);
        assert_eq!(
            ledger.read_progress(),
            None,
            "the new run starts without the previous run's counts"
        );
        assert_eq!(
            previous
                .environment_source(&checkout, ENVIRONMENT)
                .read_progress(),
            Some((3, 4)),
            "the old run remains independently readable"
        );

        // The run's own boundary publishes the host graph and resets the phase
        // in one step, before any progress poll can read it.
        let _graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        assert_eq!(
            ledger.read_progress(),
            None,
            "the previous run's counts are not projected into this one"
        );
        let published = records(&checkout, &publisher);
        for entry in ["api", "front", "admin"] {
            assert_eq!(
                state_of(&published, entry).waiting_on,
                Some(WaitOn::Environment {
                    healthy: None,
                    total: None
                }),
                "{entry} waits for the phase without inventing counts"
            );
            assert!(
                !state_of(&published, entry)
                    .status_line()
                    .contains("containers 3/4"),
                "{entry} never repeats the prior run's counts"
            );
        }

        // Only the readiness publisher's own observation reports counts.
        publisher.publish_phase(&EntryState {
            entry: "environment".to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(1),
                total: Some(4),
            }),
            not_started_because: None,
            detail: None,
            elapsed_ms: 0,
        });
        assert_eq!(ledger.read_progress(), Some((1, 4)));
    }

    /// An entry the activation's spent budget never reached is terminal and
    /// attributed, and the plan's final settle cannot turn its record into
    /// evidence of a start: a tab never announces it and status keeps the
    /// required `not started` result (effigy#458).
    #[test]
    fn a_skipped_entry_stays_not_started_through_the_plans_settle() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let all = BTreeSet::from(["api".to_owned(), "front".to_owned(), "admin".to_owned()]);
        let mut graph = HostWaitGraph::for_policy(&policy, &all, &publisher, clock.at());
        graph.withheld(
            "front",
            withheld_attribution(
                "front",
                "the activation's observation budget expired before this process was reached",
            ),
            clock.at(),
        );
        // The plan returns: every started entry settles, the skipped one stays
        // terminal.
        graph.settle_all(clock.at());

        let published = records(&checkout, &publisher);
        let front = state_of(&published, "front");
        assert_eq!(front.phase, EntryPhase::NotStarted);
        assert_eq!(
            front
                .not_started_because
                .as_ref()
                .map(|cause| cause.name.as_str()),
            Some("front")
        );
        assert!(
            front.status_line().contains("observation budget expired"),
            "{}",
            front.status_line()
        );
        let mut tab = follower(&checkout, &publisher, "front");
        tab.poll();
        assert!(
            !tab.may_announce_started(),
            "a skipped entry is never evidence of a start"
        );
        assert_eq!(
            tab.wait_state().map(EntryState::tab_label).as_deref(),
            Some("front [not started: front failed]")
        );
    }

    /// The environment's own failure is terminal for every host entry: none
    /// starts, and each names the environment and what it reported.
    #[test]
    fn an_environment_failure_marks_every_host_not_started() {
        let (_dir, checkout, publisher) = fixture();
        let policy = four_entry_policy(&checkout);
        let clock = Clock::new();
        let mut graph = HostWaitGraph::for_environment_phase(&policy, &publisher, clock.at());
        graph.environment_failed(
            "`up` exited 1 after 4s; web is unhealthy after 3 failed checks",
            clock.at() + Duration::from_secs(4),
        );
        let published = records(&checkout, &publisher);
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
        // The run's owner aborts before it starts the plan: the entries are
        // already terminal, so nothing spawns.
        let presented = frame(&checkout, &publisher, &["api", "front", "admin"], "front");
        assert!(
            presented.contains("front [not started: environment"),
            "{presented}"
        );
    }
}
