//! Complete managed-entry observations on their owning kernel run.
//!
//! A task records its workload and prelaunch host graph; the environment's
//! activation records its phase and host graph on one operation run. Readers
//! select that activation by exact parent run, never by a publisher pointer.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use effigy_core::managed_waits::{EntryPhase, EntryState};
use effigy_kernel::ctx::{EffectClass, OpenReason, Placement};
use effigy_kernel::gate::Wait;
use effigy_kernel::id::{CheckoutId, InvocationSpec, ResourceId, RunId, Scope, TaskId};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::{Invocation, WorkloadOutcome};
use effigy_kernel::run::{BeginOpts, Effect, EffectSpec, Run, Settlement};
use effigy_kernel::store::Store;
use serde::{Deserialize, Serialize};

use crate::runner::error::RunnerError;
use crate::runner::task_engine::{self, Lookup, TaskRun};

const WAIT_EFFECT: &str = "managed:waits";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Snapshot {
    environment: Option<String>,
    parent: Option<RunId>,
    entries: BTreeMap<String, EntryState>,
    // None is unobserved, distinct from a complete empty host graph.
    hosts: Option<Vec<EntryState>>,
    phase: Option<EntryState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    Run(RunId),
    Environment { name: String, parent: Option<RunId> },
}

/// A read-only selection of observations in this checkout's journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) struct WaitLedger {
    repo: PathBuf,
    source: Source,
}

impl WaitLedger {
    pub(in crate::runner) fn run(repo: &Path, run: RunId) -> Self {
        Self {
            repo: repo.to_owned(),
            source: Source::Run(run),
        }
    }

    pub(in crate::runner) fn environment(repo: &Path, environment: &str) -> Self {
        Self {
            repo: repo.to_owned(),
            source: Source::Environment {
                name: environment.to_owned(),
                parent: None,
            },
        }
    }

    pub(in crate::runner) fn for_parent(mut self, run: RunId) -> Self {
        if let Source::Environment { parent, .. } = &mut self.source {
            *parent = Some(run);
        }
        self
    }

    fn activation(&self) -> Option<Snapshot> {
        let Source::Environment { name, parent } = &self.source else {
            return None;
        };
        let checkout = task_engine::checkout::read(&self.repo).ok()??;
        let cx = task_engine::observe_context(Duration::from_secs(10));
        let root = match task_engine::observe_checkout_root(&cx, &self.repo) {
            Observed::Verified(root, _) => root,
            _ => return None,
        };
        let reader = match Store::read(&cx, &root) {
            Observed::Verified(reader, _) => reader,
            _ => return None,
        };
        let owner = activation_resource(checkout, name).ok()?;
        let runs = match effigy_kernel::run::history(&reader, &owner, task_engine::HISTORY_LIMIT) {
            Observed::Verified(runs, _) => runs,
            _ => return None,
        };
        for run in runs {
            let effects = match effigy_kernel::run::effects(&reader, run.id) {
                Observed::Verified(effects, _) => effects,
                _ => return None,
            };
            if !effects.iter().any(|effect| effect.spec.kind == WAIT_EFFECT) {
                continue;
            }
            let snapshot = read_run(&self.repo, run.id)?;
            if parent.is_none() || snapshot.parent == *parent {
                return Some(snapshot);
            }
        }
        None
    }

    fn snapshot(&self) -> Option<Snapshot> {
        match &self.source {
            Source::Run(run) => read_run(&self.repo, *run),
            Source::Environment { parent, .. } => {
                let previous = parent.and_then(|run| read_run(&self.repo, run));
                match self.activation() {
                    Some(mut current) => {
                        if current.hosts.is_none() {
                            current.hosts = previous.as_ref().and_then(|value| value.hosts.clone());
                        }
                        if current.phase.is_none() {
                            current.phase = previous.and_then(|value| value.phase);
                        }
                        Some(current)
                    }
                    None => previous,
                }
            }
        }
    }

    pub(in crate::runner) fn activation_started(&self) -> bool {
        self.activation()
            .is_some_and(|snapshot| snapshot.hosts.is_some())
    }

    pub(in crate::runner) fn read(&self, entry: &str) -> Option<EntryState> {
        self.read_all()?
            .into_iter()
            .find(|state| state.entry == entry)
    }

    /// Every entry this ledger observed, or `None` when it has no observation
    /// at all. An unobserved host graph is not a complete empty one: the
    /// environment owner publishes its host graph only once it has observed
    /// it, and a reader that treated the absence as `[]` would report a
    /// finished graph that never existed.
    pub(in crate::runner) fn read_all(&self) -> Option<Vec<EntryState>> {
        let snapshot = self.snapshot()?;
        match self.source {
            Source::Run(_) => Some(snapshot.entries.into_values().collect()),
            Source::Environment { .. } => {
                let mut hosts = snapshot.hosts?;
                hosts.sort_by(|left, right| left.entry.cmp(&right.entry));
                Some(hosts)
            }
        }
    }

    pub(in crate::runner) fn read_phase(&self) -> Option<EntryState> {
        self.snapshot()?.phase
    }
    pub(in crate::runner) fn environment_identity(&self) -> Option<String> {
        self.snapshot()?.environment
    }
    pub(in crate::runner) fn read_progress(&self) -> Option<(u32, u32)> {
        match self.read_phase()?.waiting_on? {
            effigy_core::managed_waits::WaitOn::Environment {
                healthy: Some(healthy),
                total: Some(total),
            } => Some((healthy, total)),
            _ => None,
        }
    }
}

fn from_effects(effects: &[Effect]) -> Option<Snapshot> {
    let latest = effects
        .iter()
        .rev()
        .find(|effect| effect.spec.kind == WAIT_EFFECT)?;
    serde_json::from_value(latest.spec.payload.clone()).ok()
}

fn read_run(repo: &Path, run: RunId) -> Option<Snapshot> {
    let Lookup::Found(record) = task_engine::lookup_checkout(repo, run) else {
        return None;
    };
    let effects = match record.effects() {
        Observed::Verified(effects, _) => effects,
        _ => return None,
    };
    let mut snapshot = from_effects(&effects)?;
    if record.terminal() {
        for state in snapshot
            .entries
            .values_mut()
            .chain(snapshot.hosts.iter_mut().flatten())
            .chain(snapshot.phase.iter_mut())
        {
            if matches!(state.phase, EntryPhase::Waiting | EntryPhase::Starting) {
                state.phase = EntryPhase::Failed;
                state.waiting_on = None;
                state.detail = Some("wait owner ended without a terminal observation".to_owned());
            }
        }
    }
    Some(snapshot)
}

/// The actual run owner writes one atomic snapshot. Clones share only its
/// in-memory model; the weak handle cannot keep a finished owner alive.
#[derive(Clone)]
pub(in crate::runner) struct WaitPublisher {
    run: Weak<Mutex<Run<'static>>>,
    snapshot: Arc<Mutex<Snapshot>>,
    /// The first observation the owner could not record. It is kept rather
    /// than discarded so the run's owner reports an incomplete wait journal
    /// instead of a complete one.
    recording_failure: Arc<Mutex<Option<String>>>,
    #[cfg(test)]
    _fixture_owner: Option<Arc<Mutex<Run<'static>>>>,
}

impl std::fmt::Debug for WaitPublisher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WaitPublisher").finish_non_exhaustive()
    }
}

impl WaitPublisher {
    pub(in crate::runner) fn from_run(
        run: &TaskRun,
        environment: Option<&str>,
    ) -> Result<Self, RunnerError> {
        Ok(Self::new(run.journal_handle()?, environment, None))
    }

    fn new(
        run: Weak<Mutex<Run<'static>>>,
        environment: Option<&str>,
        parent: Option<RunId>,
    ) -> Self {
        Self {
            run,
            snapshot: Arc::new(Mutex::new(Snapshot {
                environment: environment.map(str::to_owned),
                parent,
                ..Snapshot::default()
            })),
            recording_failure: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            _fixture_owner: None,
        }
    }

    /// The first observation this publisher could not record, if any. A run
    /// owner that reads `Some` must not report its wait journal as complete.
    pub(in crate::runner) fn take_recording_failure(&self) -> Option<String> {
        self.recording_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
    }

    fn record_failure(&self, failure: impl std::fmt::Display) {
        let mut recorded = self
            .recording_failure
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if recorded.is_none() {
            *recorded = Some(failure.to_string());
        }
    }

    #[cfg(test)]
    pub(in crate::runner) fn run_id(&self) -> RunId {
        self.run
            .upgrade()
            .expect("fixture observation owner")
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .id()
    }
    #[cfg(test)]
    pub(in crate::runner) fn source(&self, repo: &Path) -> WaitLedger {
        WaitLedger::run(repo, self.run_id())
    }
    #[cfg(test)]
    pub(in crate::runner) fn environment_source(&self, repo: &Path, name: &str) -> WaitLedger {
        WaitLedger::environment(repo, name).for_parent(self.run_id())
    }

    fn update(&self, change: impl FnOnce(&mut Snapshot)) {
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        change(&mut snapshot);
        let Some(run) = self.run.upgrade() else {
            // The owner is gone, so this observation has no journal to live
            // in. Name that instead of dropping it without trace.
            self.record_failure("the managed wait owner is gone; the observation was not recorded");
            return;
        };
        let mut run = run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(payload) = serde_json::to_value(&*snapshot) else {
            return;
        };
        // These are already-observed facts, so intent atomically publishes the
        // complete payload. No reader waits for an applied receipt to decode it.
        let spec = EffectSpec {
            target: run.fence().clone(),
            kind: WAIT_EFFECT.to_owned(),
            payload,
            settlement: Settlement::Retain,
            required: false,
        };
        let ticket = match run.intend(spec) {
            Ok(ticket) => ticket,
            Err(error) => {
                self.record_failure(format!(
                    "the managed wait observation could not be recorded: {error}"
                ));
                return;
            }
        };
        // The intent already carries the complete payload, so readers decode
        // the observation even if the receipt write fails. A failed receipt is
        // still a fact about this journal: settle it as unknown rather than
        // leave it reading as applied, and report both the receipt failure and
        // any failure to settle, so neither is discarded.
        if let Err(error) = run.applied(&ticket, serde_json::Value::Null) {
            let mut failure =
                format!("the managed wait observation receipt could not be written: {error}");
            if let Err(settled) = run.unknown(
                &ticket,
                effigy_kernel::observe::UnknownReason::Inconsistent(format!(
                    "the observation receipt could not be written: {error}"
                )),
            ) {
                failure.push_str(&format!(
                    "; it could not be recorded as unknown either: {settled}"
                ));
            }
            self.record_failure(failure);
        }
    }

    pub(in crate::runner) fn publish_entries(&self, states: &[EntryState]) {
        self.update(|snapshot| {
            for state in states {
                snapshot.entries.insert(state.entry.clone(), state.clone());
            }
        });
    }
    pub(in crate::runner) fn publish(&self, state: &EntryState) {
        self.publish_entries(std::slice::from_ref(state));
    }
    pub(in crate::runner) fn settle(&self, entry: &str) {
        self.publish(&settled(entry));
    }
    pub(in crate::runner) fn publish_hosts(&self, states: &[EntryState]) {
        self.update(|snapshot| snapshot.hosts = Some(states.to_vec()));
    }
    pub(in crate::runner) fn publish_environment(&self, phase: &EntryState, hosts: &[EntryState]) {
        self.update(|snapshot| {
            snapshot.phase = Some(phase.clone());
            snapshot.hosts = Some(hosts.to_vec());
        });
    }

    pub(in crate::runner) fn publish_phase(&self, state: &EntryState) {
        self.update(|snapshot| snapshot.phase = Some(state.clone()));
    }
    pub(in crate::runner) fn settle_phase(&self) {
        self.publish_phase(&settled("environment"));
    }
    pub(in crate::runner) fn publish_environment_identity(&self, environment: &str) {
        self.update(|snapshot| snapshot.environment = Some(environment.to_owned()));
    }

    #[cfg(test)]
    pub(in crate::runner) fn fixture(repo: &Path, environment: Option<&str>) -> Self {
        let run = begin_run(repo, environment.unwrap_or("fixture"), true)
            .expect("fixture observation run");
        let mut publisher = Self::new(Arc::downgrade(&run), environment, None);
        publisher._fixture_owner = Some(run);
        publisher
    }

    #[cfg(test)]
    pub(in crate::runner) fn fixture_child(repo: &Path, environment: &str, parent: RunId) -> Self {
        let run = begin_run(repo, environment, false).expect("fixture activation run");
        let mut publisher = Self::new(Arc::downgrade(&run), Some(environment), Some(parent));
        publisher._fixture_owner = Some(run);
        publisher
    }
}

pub(in crate::runner) fn settled(entry: &str) -> EntryState {
    EntryState {
        entry: entry.to_owned(),
        phase: EntryPhase::Ready,
        waiting_on: None,
        not_started_because: None,
        detail: None,
        elapsed_ms: 0,
    }
}

fn activation_resource(checkout: CheckoutId, environment: &str) -> Result<ResourceId, RunnerError> {
    ResourceId::store(
        Scope::Checkout(checkout),
        &format!("host-activation:{environment}"),
    )
    .map_err(error)
}

fn error(error: impl std::fmt::Display) -> RunnerError {
    RunnerError::task_invocation(error.to_string())
}

fn begin_run(
    repo: &Path,
    environment: &str,
    fixture: bool,
) -> Result<Arc<Mutex<Run<'static>>>, RunnerError> {
    let checkout = task_engine::checkout::mint(repo)?;
    let id = task_engine::fresh_run_id()?;
    let owner = if fixture {
        ResourceId::store(Scope::Checkout(checkout), &format!("wait-fixture:{id}"))
            .map_err(error)?
    } else {
        activation_resource(checkout, environment)?
    };
    let task = TaskId::new(
        checkout,
        "",
        effigy_kernel::id::Surface::Published,
        "host-activation",
        Some(environment),
    )
    .map_err(error)?;
    let argv = vec![
        "effigy".into(),
        "container".into(),
        environment.into(),
        "up".into(),
    ];
    let invocation = effigy_kernel::id::ResolvedInvocation::new(InvocationSpec {
        task,
        route: effigy_kernel::id::Route::Task,
        argv: argv.clone(),
        cwd: String::new(),
        target: effigy_kernel::id::ExecutionTarget::Host,
        env: BTreeMap::new(),
        owner,
        gates: Vec::new(),
        attachments: Vec::new(),
        class: EffectClass::Mutate,
        placement: Placement::Host,
        provenance: BTreeMap::new(),
    })
    .map_err(error)?;
    let next = Invocation::validated(argv, |args| {
        args.len() == 4 && args[0] == "effigy" && args[1] == "container" && args[3] == "up"
    })
    .map_err(|_| error("invalid activation recovery invocation"))?;
    let cx = task_engine::effect_context(OpenReason::Shell);
    let cleanup = task_engine::cleanup_context(Duration::from_secs(10));
    let run = Run::begin(
        &cx,
        task_engine::checkout_effects_root(repo)?,
        &invocation,
        BeginOpts {
            id,
            wait: Wait::FailFast,
            acknowledge: Vec::new(),
            next,
            custody: None,
        },
        &cleanup,
        &mut task_engine::TaskSettlement::default(),
    )
    .map_err(error)?;
    Ok(Arc::new(Mutex::new(run)))
}

/// The existing activation/planning operation, journaled without holding Env
/// across readiness. Only observations belong to this run.
pub(in crate::runner) struct ActivationWaits {
    run: Option<Arc<Mutex<Run<'static>>>>,
    publisher: WaitPublisher,
}

impl std::fmt::Debug for ActivationWaits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActivationWaits")
            .field("open", &self.run.is_some())
            .finish()
    }
}

impl ActivationWaits {
    pub(in crate::runner) fn begin(
        repo: &Path,
        environment: &str,
        parent: Option<RunId>,
    ) -> Result<Self, RunnerError> {
        let run = begin_run(repo, environment, false)?;
        let publisher = WaitPublisher::new(Arc::downgrade(&run), Some(environment), parent);
        Ok(Self {
            run: Some(run),
            publisher,
        })
    }

    pub(in crate::runner) fn publisher(&self) -> &WaitPublisher {
        &self.publisher
    }

    pub(in crate::runner) fn finish(&mut self, success: bool) -> Result<(), RunnerError> {
        let closed = self.close(if success {
            WorkloadOutcome::Succeeded
        } else {
            WorkloadOutcome::Failed { code: 1 }
        });
        match self.publisher.take_recording_failure() {
            Some(failure) => closed.and(Err(error(failure))),
            None => closed,
        }
    }

    fn close(&mut self, outcome: WorkloadOutcome) -> Result<(), RunnerError> {
        let Some(run) = self.run.take() else {
            return Ok(());
        };
        let mut run = Arc::try_unwrap(run)
            .map_err(|_| error("activation observation owner is still borrowed"))?
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        run.finish(outcome).map_err(error)?;
        run.settle(
            &task_engine::cleanup_context(Duration::from_secs(10)),
            &mut task_engine::TaskSettlement::default(),
        )
        .map_err(error)?;
        Ok(())
    }
}

impl Drop for ActivationWaits {
    fn drop(&mut self) {
        if self.run.is_none() {
            return;
        }
        // A drop is the path taken when the caller never reached `finish`:
        // a panic or an early return. The run is settled as interrupted so its
        // journal is not left intended, and a failure to settle an observation
        // is reported rather than discarded — a silently lost wait is exactly
        // the incomplete record this type exists to prevent.
        let settled = self.close(WorkloadOutcome::Interrupted);
        let failure = self
            .publisher
            .take_recording_failure()
            .map(error)
            .or_else(|| settled.err());
        if let Some(failure) = failure {
            eprintln!("warning: {failure}");
        }
    }
}

pub(in crate::runner) fn with_activation_waits<T>(
    repo: &Path,
    environment: &str,
    parent: Option<RunId>,
    work: impl FnOnce(WaitPublisher) -> Result<T, RunnerError>,
) -> Result<T, RunnerError> {
    let mut activation = ActivationWaits::begin(repo, environment, parent)?;
    let result = work(activation.publisher().clone());
    activation.finish(result.is_ok())?;
    result
}

/// Injective component encoding, also used by exec markers.
pub(in crate::runner) fn path_component(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use effigy_core::managed_waits::{Attribution, WaitOn};

    fn waiting(name: &str) -> EntryState {
        EntryState {
            entry: name.to_owned(),
            phase: EntryPhase::Waiting,
            waiting_on: Some(WaitOn::Probe {
                probe: "GET /health -> 200".to_owned(),
            }),
            not_started_because: None,
            detail: Some("observed detail".to_owned()),
            elapsed_ms: 17,
        }
    }

    #[test]
    fn one_snapshot_keeps_distinct_names_and_the_phase_separate() {
        let repo = tempfile::tempdir().unwrap();
        let publisher = WaitPublisher::fixture(repo.path(), Some("app/one"));
        let names = [
            "a+b",
            "a/b",
            "a%2Fb",
            "a b",
            "a-b",
            "environment",
            "@environment",
            "雪",
        ];
        let entries = names.map(waiting);
        publisher.publish_entries(&entries);
        publisher.publish_environment(&waiting("environment"), &entries);
        let task = publisher.source(repo.path());
        let hosts = publisher.environment_source(repo.path(), "app/one");
        for entry in &entries {
            assert_eq!(task.read(&entry.entry).as_ref(), Some(entry));
            assert_eq!(hosts.read(&entry.entry).as_ref(), Some(entry));
        }
        let mut ordered = entries.to_vec();
        ordered.sort_by(|left, right| left.entry.cmp(&right.entry));
        assert_eq!(task.read_all(), Some(ordered.clone()));
        assert_eq!(hosts.read_all(), Some(ordered));
        assert_eq!(hosts.read_phase(), Some(waiting("environment")));
        assert_eq!(task.environment_identity().as_deref(), Some("app/one"));
        assert!(!repo.path().join(".effigy/runtime/waits").exists());
        let encoded = names
            .map(path_component)
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            encoded.len(),
            names.len(),
            "exec marker names remain injective too"
        );
    }

    #[test]
    fn exact_runs_never_share_waits_and_a_new_run_has_no_old_counts() {
        let repo = tempfile::tempdir().unwrap();
        let first = WaitPublisher::fixture(repo.path(), Some("app"));
        let second = WaitPublisher::fixture(repo.path(), Some("app"));
        first.publish(&waiting("first"));
        first.publish_phase(&EntryState {
            waiting_on: Some(WaitOn::Environment {
                healthy: Some(1),
                total: Some(4),
            }),
            ..waiting("environment")
        });
        assert_ne!(first.run_id(), second.run_id());
        assert_eq!(
            second.source(repo.path()).read_all(),
            None,
            "a run that never observed an entry has no observed graph at all"
        );
        assert_eq!(second.source(repo.path()).read_progress(), None);
        second.publish(&waiting("second"));
        assert!(first.source(repo.path()).read("second").is_none());
        assert!(second.source(repo.path()).read("first").is_none());
    }

    #[test]
    fn attribution_survives_its_owner_without_turning_an_unobserved_launch_ready() {
        let repo = tempfile::tempdir().unwrap();
        let publisher = WaitPublisher::fixture(repo.path(), None);
        let source = publisher.source(repo.path());
        let failure = EntryState {
            phase: EntryPhase::NotStarted,
            waiting_on: None,
            not_started_because: Some(Attribution::new("api", "failed (status 500)")),
            ..waiting("front")
        };
        publisher.publish_entries(&[failure.clone(), waiting("unobserved")]);
        drop(publisher);
        assert_eq!(source.read("front"), Some(failure));
        let unknown = source.read("unobserved").unwrap();
        assert_eq!(unknown.phase, EntryPhase::Failed);
        assert!(unknown.waiting_on.is_none());
        assert!(unknown
            .detail
            .unwrap()
            .contains("without a terminal observation"));
    }

    #[test]
    fn an_unobserved_host_graph_reads_as_absent_not_as_a_complete_empty_one() {
        let repo = tempfile::tempdir().unwrap();
        let publisher = WaitPublisher::fixture(repo.path(), Some("app"));
        let empty = publisher.environment_source(repo.path(), "app");
        // Publishing nothing leaves no observation at all; it is not the same
        // fact as an environment whose host graph is complete and empty.
        assert_eq!(empty.read_all(), None);
        assert_eq!(empty.read("api"), None);
        publisher.publish_hosts(&[]);
        assert_eq!(
            publisher.environment_source(repo.path(), "app").read_all(),
            Some(Vec::new())
        );
    }

    #[test]
    fn an_observation_the_journal_refuses_is_reported_not_discarded() {
        let repo = tempfile::tempdir().unwrap();
        let mut activation = ActivationWaits::begin(repo.path(), "app", None).unwrap();
        let publisher = activation.publisher().clone();
        publisher.publish(&waiting("api"));
        assert!(publisher.take_recording_failure().is_none());
        activation.finish(true).unwrap();
        // The owner has settled: a later observation cannot be recorded, and
        // the publisher must name that instead of dropping it silently.
        publisher.publish(&waiting("late"));
        let failure = publisher
            .take_recording_failure()
            .expect("a refused observation is reported");
        assert!(failure.contains("not recorded"), "{failure}");
    }

    #[test]
    fn latest_observation_is_atomic_before_receipt_and_malformed_never_replays_old_waits() {
        let repo = tempfile::tempdir().unwrap();
        let publisher = WaitPublisher::fixture(repo.path(), None);
        publisher.publish(&waiting("api"));
        let Lookup::Found(record) = task_engine::lookup_checkout(repo.path(), publisher.run_id())
        else {
            panic!("fixture run")
        };
        let Observed::Verified(mut effects, _) = record.effects() else {
            panic!("fixture effects")
        };
        let latest = effects.last_mut().unwrap();
        latest.state = effigy_kernel::run::EffectState::Intended;
        latest.receipt = None;
        assert_eq!(
            from_effects(&effects).unwrap().entries["api"],
            waiting("api")
        );
        let mut malformed = effects.last().unwrap().clone();
        malformed.spec.payload = serde_json::json!({});
        effects.push(malformed);
        assert!(from_effects(&effects).is_none());
    }
}
