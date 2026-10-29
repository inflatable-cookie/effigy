//! Every managed session entry's own wait, derived from the resolved ordering
//! graph (effigy#458, Q-017).
//!
//! A managed session's entries are ordered by the plan, not by the tab bar: a
//! workload waits for the entry the resolved start order puts before it and
//! for the environment phase, an environment host process waits for the
//! `depends_on` entries it declares, and the accepted legacy `start_after_ms`
//! delays one entry without naming another. This module is the one place that
//! turns those facts into a per-entry phase and wait, so the TUI tab, the
//! text/headless lines and the JSON record all say the same thing.
//!
//! The rules that keep a label honest:
//!
//! - a waiting entry names the exact thing it waits for: the dependency and
//!   that dependency's current phase, the environment and the containers that
//!   are healthy, or the readiness probe with its elapsed time;
//! - a delay that orders nothing names itself and what remains of it, never
//!   another entry's label;
//! - a definitive prerequisite failure is terminal and attributed: every
//!   dependent is `not_started` with the dependency and what it reported, and
//!   it may not start;
//! - elapsed time belongs to the entry's own wait, so one entry's clock never
//!   appears on another entry.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// The schema of one entry's published wait record. A record is what a front
/// end follows when the process that owns the wait is another process.
pub const ENTRY_WAIT_SCHEMA: &str = "effigy.managed.entry-wait.v1";

/// How often a live wait's published elapsed time is refreshed. A wait whose
/// meaning has not changed is not rewritten more often than this.
pub const WAIT_REFRESH: std::time::Duration = std::time::Duration::from_millis(500);

/// One entry's phase in its session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryPhase {
    /// Not started yet: it is blocked by what `waiting_on` names.
    Waiting,
    /// Its process has been launched and has not reported running or ready.
    Starting,
    /// Its process is running; a listener is not serving yet.
    Running,
    /// A listener published its route; a plain process reached running.
    Ready,
    /// A prerequisite failed, so this entry never started.
    NotStarted,
    /// Its own start failed.
    Failed,
    /// It started and its process exited.
    Exited,
}

impl EntryPhase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Ready => "ready",
            Self::NotStarted => "not_started",
            Self::Failed => "failed",
            Self::Exited => "exited",
        }
    }

    /// Whether this entry's process has started, so a dependent that waits for
    /// its start may start too.
    pub fn started(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Running | Self::Ready | Self::Failed | Self::Exited
        )
    }

    /// Whether this entry can never start.
    pub fn blocked(self) -> bool {
        matches!(self, Self::NotStarted)
    }
}

/// What one entry waits for, named exactly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WaitOn {
    /// The run's environment phase, with the containers that are healthy when
    /// the phase reports them.
    Environment {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        healthy: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        total: Option<u32>,
    },
    /// Another entry of the same session, and the phase it is in now.
    Entry { name: String, phase: EntryPhase },
    /// The scheduler launches the session's entries one at a time: this
    /// entry's start is held by the entry the plan is launching now. It is a
    /// real per-entry wait, not a declared dependency, so it is never
    /// attributed as one (effigy#458).
    Launch { name: String, phase: EntryPhase },
    /// A readiness probe of this entry itself.
    Probe { probe: String },
    /// The accepted legacy delay: no dependency is named, so the wait names
    /// itself and what remains.
    Delay { remaining_ms: u64 },
}

impl WaitOn {
    /// The dependency this wait names, when it names one.
    pub fn dependency(&self) -> Option<(&str, EntryPhase)> {
        match self {
            Self::Entry { name, phase } => Some((name.as_str(), *phase)),
            _ => None,
        }
    }

    /// The wait in the session's own words, with its elapsed time where the
    /// wait has one.
    pub fn text(&self, elapsed: Duration) -> String {
        match self {
            Self::Environment { healthy, total } => match (healthy, total) {
                (Some(healthy), Some(total)) => {
                    format!("waiting for environment (containers {healthy}/{total} healthy)")
                }
                _ => "waiting for environment".to_owned(),
            },
            Self::Entry { name, phase } => {
                format!("waiting for {name} ({})", phase.as_str())
            }
            Self::Launch { name, phase } => {
                format!("waiting for {name}'s launch ({})", phase.as_str())
            }
            Self::Probe { probe } => format!("waiting for {probe} ({})", seconds(elapsed)),
            Self::Delay { remaining_ms } => {
                format!(
                    "scheduled delay: {}s remaining",
                    whole_seconds(*remaining_ms)
                )
            }
        }
    }

    /// The same wait, short enough for a tab title.
    pub fn short(&self) -> String {
        match self {
            Self::Environment { .. } => "environment".to_owned(),
            Self::Entry { name, phase } => format!("{name} {}", phase.as_str()),
            Self::Launch { name, .. } => format!("{name} launching"),
            Self::Probe { probe } => probe.clone(),
            Self::Delay { remaining_ms } => format!("{}s delay", whole_seconds(*remaining_ms)),
        }
    }
}

/// The prerequisite whose failure stopped an entry from starting, and what the
/// prerequisite reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    pub name: String,
    pub detail: String,
}

impl Attribution {
    pub fn new(name: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            detail: detail.into(),
        }
    }

    /// `api failed (status 500)`.
    pub fn text(&self) -> String {
        format!("{} {}", self.name, self.detail)
    }
}

/// One entry's state, as every front end renders it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryState {
    pub entry: String,
    pub phase: EntryPhase,
    /// What it waits for while it has not started, or while it is starting and
    /// its own readiness probe has not answered.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_on: Option<WaitOn>,
    /// Why it never started: the prerequisite and what that prerequisite
    /// reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not_started_because: Option<Attribution>,
    /// What the entry itself reported when it failed or exited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// How long the current phase or wait has lasted.
    pub elapsed_ms: u64,
}

impl EntryState {
    /// The tab title: the entry's own name, then its own state.
    pub fn tab_label(&self) -> String {
        let state = match self.phase {
            EntryPhase::Waiting => match &self.waiting_on {
                Some(wait) => format!("waiting: {}", wait.short()),
                None => "waiting".to_owned(),
            },
            EntryPhase::Starting => match &self.waiting_on {
                Some(WaitOn::Probe { probe }) => {
                    format!("starting: {probe} ({}s)", self.elapsed_ms / 1000)
                }
                _ => "starting".to_owned(),
            },
            EntryPhase::Running => "running".to_owned(),
            EntryPhase::Ready => "ready".to_owned(),
            EntryPhase::NotStarted => match &self.not_started_because {
                Some(cause) => format!("not started: {} failed", cause.name),
                None => "not started".to_owned(),
            },
            EntryPhase::Failed => "failed".to_owned(),
            EntryPhase::Exited => "exited".to_owned(),
        };
        format!("{} [{state}]", self.entry)
    }

    /// The line a pane or a headless run shows for this entry.
    pub fn status_line(&self) -> String {
        let elapsed = Duration::from_millis(self.elapsed_ms);
        match self.phase {
            EntryPhase::Waiting => match &self.waiting_on {
                Some(WaitOn::Delay { remaining_ms }) => WaitOn::Delay {
                    remaining_ms: *remaining_ms,
                }
                .text(elapsed),
                Some(WaitOn::Probe { .. }) => {
                    self.waiting_on.as_ref().expect("probe").text(elapsed)
                }
                Some(wait) => format!("{} ({})", wait.text(elapsed), seconds(elapsed)),
                None => "waiting to start".to_owned(),
            },
            EntryPhase::Starting => match &self.waiting_on {
                Some(wait) => wait.text(elapsed),
                None => format!("starting ({})", seconds(elapsed)),
            },
            EntryPhase::Running => format!("running ({})", seconds(elapsed)),
            EntryPhase::Ready => "ready".to_owned(),
            EntryPhase::NotStarted => match &self.not_started_because {
                Some(cause) => format!("not started — {}", cause.text()),
                None => "not started".to_owned(),
            },
            EntryPhase::Failed => match &self.detail {
                Some(detail) => format!("failed: {detail}"),
                None => "failed".to_owned(),
            },
            EntryPhase::Exited => match &self.detail {
                Some(detail) => format!("exited {detail}"),
                None => "exited".to_owned(),
            },
        }
    }

    /// Whether this entry has not started and is waiting for someone else.
    pub fn is_waiting(&self) -> bool {
        self.phase == EntryPhase::Waiting
    }

    /// Whether this state is a wait a front end renders, rather than a phase
    /// the entry's own process state already tells: it has not started, it
    /// never will, or it is starting against a named wait.
    pub fn holds_a_wait(&self) -> bool {
        matches!(
            self.phase,
            EntryPhase::Waiting | EntryPhase::NotStarted | EntryPhase::Failed
        ) || (self.phase == EntryPhase::Starting && self.waiting_on.is_some())
    }

    /// Whether two states mean the same thing. Elapsed time is excluded: it
    /// advances on its own and is refreshed by [`should_publish`], so a
    /// settled record is never rewritten just because it aged.
    pub fn means_the_same_as(&self, other: &Self) -> bool {
        self.entry == other.entry
            && self.phase == other.phase
            && self.waiting_on == other.waiting_on
            && self.not_started_because == other.not_started_because
            && self.detail == other.detail
    }

    /// The line a headless/text front end prints for this state, when it is a
    /// wait. `None` once the entry has started, or while it waits for nothing
    /// it can name yet, so a settled or empty wait never repeats.
    pub fn headless_line(&self) -> Option<String> {
        let says_something = self.holds_a_wait()
            && !(self.phase == EntryPhase::Waiting && self.waiting_on.is_none());
        says_something.then(|| format!("[managed] {}: {}", self.entry, self.status_line()))
    }
}

/// Whether a writer must publish this state now: its meaning changed, or it is
/// a live wait whose elapsed time is due for a refresh.
///
/// A started process or a settled record only ages, so it is written once and
/// then left alone however often the owner loop turns (effigy#458).
pub fn should_publish(
    previous: Option<&EntryState>,
    published_at: Option<Instant>,
    state: &EntryState,
    now: Instant,
) -> bool {
    if previous.is_none_or(|previous| !previous.means_the_same_as(state)) {
        return true;
    }
    let live = state.waiting_on.is_some()
        && matches!(state.phase, EntryPhase::Waiting | EntryPhase::Starting);
    live && published_at.is_some_and(|at| now.saturating_duration_since(at) >= WAIT_REFRESH)
}

/// The probe label of a host listener's declared readiness request:
/// `GET / -> 200`. It names the request the wait observes, not the whole URL.
pub fn listener_probe_label(path: &str, status: u16) -> String {
    let path = if path.trim().is_empty() { "/" } else { path };
    format!("GET {path} -> {status}")
}

/// One entry's declared ordering inputs, resolved from the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderEntry {
    pub name: String,
    /// The entries that must be satisfied before this one starts.
    pub waits_for: Vec<String>,
    /// Whether `waits_for` names host listeners whose *readiness* is required
    /// (`depends_on`), rather than entries that must merely have started.
    pub waits_for_readiness: bool,
    /// Whether this entry starts only after the run's environment phase.
    pub after_environment: bool,
    /// The entry whose launch holds this one: the session launches its entries
    /// one at a time, so every later entry waits for the one before it even
    /// when it declares no dependency of its own (effigy#458).
    pub held_by: Option<String>,
}

/// One workload's ordering inputs, as the resolved managed plan states them.
#[derive(Debug, Clone, Copy)]
pub struct WorkloadOrder<'a> {
    pub name: &'a str,
    /// The entry's declared `after`; the one accepted value is `environment`.
    /// It states the environment relationship rather than changing the order,
    /// and the accepted legacy `start_after_ms` delay is fed while the run
    /// holds the entry (`EntryWaits::delaying`), never resolved into the graph.
    pub after: &'a [String],
}

/// One environment host process's ordering inputs.
#[derive(Debug, Clone, Copy)]
pub struct HostOrder<'a> {
    pub name: &'a str,
    /// The process's declared `depends_on`: readers that must be ready first.
    pub depends_on: &'a [String],
}

/// The session's resolved ordering graph: every entry and what it waits for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionOrder {
    entries: Vec<OrderEntry>,
}

impl SessionOrder {
    pub fn new(entries: Vec<OrderEntry>) -> Self {
        Self { entries }
    }

    /// The workload graph: the supervisor starts workloads one at a time in
    /// the plan's resolved start order, so each entry waits for the entry
    /// before it, and every workload waits for the environment phase that
    /// precedes all of them.
    pub fn workloads(entries: &[WorkloadOrder<'_>]) -> Self {
        let entries = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| OrderEntry {
                name: entry.name.to_owned(),
                waits_for: entries
                    .get(index.wrapping_sub(1))
                    .filter(|_| index > 0)
                    .map(|previous| vec![previous.name.to_owned()])
                    .unwrap_or_default(),
                waits_for_readiness: false,
                // The environment phase always precedes every workload; an
                // entry's `after = ["environment"]` states that relationship
                // rather than changing the order.
                after_environment: true,
                // The run's supervisor sequences workloads itself and names
                // that wait, so the graph adds no second hold.
                held_by: None,
            })
            .collect();
        Self { entries }
    }

    /// The environment's host-process graph: each process waits first for the
    /// environment phase and then for the readers its own `depends_on`
    /// declares, and for their readiness.
    ///
    /// A bring-up starts host processes after the environment's services
    /// report ready, so the phase is a real wait of theirs. The planner marks
    /// the phase ready before it starts them; while the environment is still
    /// coming up the same graph names it instead.
    pub fn hosts(entries: &[HostOrder<'_>]) -> Self {
        let entries = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| OrderEntry {
                name: entry.name.to_owned(),
                waits_for: entry.depends_on.to_vec(),
                waits_for_readiness: true,
                after_environment: true,
                // The plan launches one entry at a time: each one is held by
                // the entry before it until that entry's launch and readiness
                // observation are over.
                held_by: index
                    .checked_sub(1)
                    .map(|previous| entries[previous].name.to_owned()),
            })
            .collect();
        Self { entries }
    }

    pub fn entries(&self) -> &[OrderEntry] {
        &self.entries
    }

    pub fn get(&self, name: &str) -> Option<&OrderEntry> {
        self.entries.iter().find(|entry| entry.name == name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.name.as_str())
    }
}

/// The environment phase as the session observes it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Environment {
    state: EntryPhase,
    healthy: Option<u32>,
    total: Option<u32>,
    since: Instant,
}

/// One entry's recorded state.
#[derive(Debug, Clone)]
struct Record {
    phase: EntryPhase,
    since: Instant,
    /// The wait that is not derived from the graph: a readiness probe or a
    /// scheduled delay.
    explicit: Option<WaitOn>,
    /// The graph-derived wait this entry showed last, so its clock restarts
    /// when that wait changes.
    last_derived: Option<WaitOn>,
    wait_since: Instant,
    detail: Option<String>,
    not_started_because: Option<Attribution>,
}

impl Record {
    fn new(now: Instant) -> Self {
        Self {
            phase: EntryPhase::Waiting,
            since: now,
            explicit: None,
            last_derived: None,
            wait_since: now,
            detail: None,
            not_started_because: None,
        }
    }
}

/// Every entry's wait in one session, advanced by whoever owns the waits.
///
/// The owner records transitions (a phase change, a probe, a delay, a failure)
/// and asks for `states`, so a front end never invents a label from screen
/// order or a timer.
#[derive(Debug, Clone)]
pub struct EntryWaits {
    order: SessionOrder,
    environment: Option<Environment>,
    records: BTreeMap<String, Record>,
}

impl EntryWaits {
    /// A session over `order`. `environment` names the run's environment phase
    /// when the run owns or borrows one: every entry that waits for it is
    /// waiting until `environment_ready`.
    pub fn new(order: SessionOrder, environment: Option<&str>, now: Instant) -> Self {
        let environment = environment.map(|_| Environment {
            state: EntryPhase::Waiting,
            healthy: None,
            total: None,
            since: now,
        });
        let records = order
            .entries()
            .iter()
            .map(|entry| (entry.name.clone(), Record::new(now)))
            .collect();
        Self {
            order,
            environment,
            records,
        }
    }

    pub fn order(&self) -> &SessionOrder {
        &self.order
    }

    /// The environment phase reports the containers that are healthy now.
    pub fn environment_progress(&mut self, healthy: u32, total: u32) {
        if let Some(environment) = self.environment.as_mut() {
            if environment.state == EntryPhase::Waiting {
                environment.healthy = Some(healthy);
                environment.total = Some(total);
            }
        }
    }

    pub fn environment_ready(&mut self, now: Instant) {
        if let Some(environment) = self.environment.as_mut() {
            if environment.state != EntryPhase::Ready {
                environment.state = EntryPhase::Ready;
                environment.since = now;
                environment.healthy = environment.total;
            }
        }
    }

    /// The environment phase failed: every workload that waits for it is
    /// `not started` with the environment's own report.
    pub fn environment_failed(&mut self, detail: &str, now: Instant) {
        if let Some(environment) = self.environment.as_mut() {
            environment.state = EntryPhase::Failed;
            environment.since = now;
        }
        let dependents = self
            .order
            .entries()
            .iter()
            .filter(|entry| entry.after_environment)
            .map(|entry| entry.name.clone())
            .collect::<Vec<_>>();
        for name in dependents {
            self.mark_not_started(&name, Attribution::new("environment", detail), now);
        }
    }

    /// The entry's process was launched.
    pub fn started(&mut self, entry: &str, now: Instant) {
        self.transition(entry, EntryPhase::Starting, now);
    }

    /// The entry's process is running.
    pub fn running(&mut self, entry: &str, now: Instant) {
        self.transition(entry, EntryPhase::Running, now);
    }

    /// A listener reported ready; a plain process reached its running state.
    pub fn ready(&mut self, entry: &str, now: Instant) {
        self.transition(entry, EntryPhase::Ready, now);
    }

    /// The entry's own start failed, with what it reported.
    pub fn failed(&mut self, entry: &str, detail: impl Into<String>, now: Instant) {
        let detail = detail.into();
        if let Some(record) = self.records.get_mut(entry) {
            record.phase = EntryPhase::Failed;
            record.since = now;
            record.explicit = None;
            record.detail = Some(detail.clone());
        }
        self.fail_dependents(entry, Attribution::new(entry, detail), now);
    }

    /// The entry's process exited with `code`.
    pub fn exited(&mut self, entry: &str, code: i32, now: Instant) {
        if let Some(record) = self.records.get_mut(entry) {
            record.phase = EntryPhase::Exited;
            record.since = now;
            record.explicit = None;
            record.detail = Some(format!("({code})"));
        }
        if code != 0 {
            // A dependent that needed this entry only started cannot be
            // stopped by a later exit; a dependent that needed its readiness
            // never gets it, so it is not started and attributed.
            self.fail_dependents(
                entry,
                Attribution::new(entry, format!("exited with {code}")),
                now,
            );
        }
    }

    /// This entry is waiting on its own readiness probe.
    pub fn probing(&mut self, entry: &str, probe: impl Into<String>, now: Instant) {
        self.set_explicit(
            entry,
            WaitOn::Probe {
                probe: probe.into(),
            },
            now,
        );
    }

    /// The accepted legacy delay: how long this entry's start is still held.
    pub fn delaying(&mut self, entry: &str, remaining: Duration, now: Instant) {
        let remaining_ms = u64::try_from(remaining.as_millis()).unwrap_or(u64::MAX);
        let Some(record) = self.records.get_mut(entry) else {
            return;
        };
        if record.phase != EntryPhase::Waiting {
            return;
        }
        if !matches!(record.explicit, Some(WaitOn::Delay { .. })) {
            record.wait_since = now;
        }
        record.explicit = Some(WaitOn::Delay {
            remaining_ms: remaining_ms.max(1),
        });
    }

    /// A delay that no longer holds the entry.
    pub fn delay_elapsed(&mut self, entry: &str, now: Instant) {
        if let Some(record) = self.records.get_mut(entry) {
            if matches!(record.explicit, Some(WaitOn::Delay { .. })) {
                record.explicit = None;
                record.wait_since = now;
            }
        }
    }

    fn set_explicit(&mut self, entry: &str, wait: WaitOn, now: Instant) {
        if let Some(record) = self.records.get_mut(entry) {
            if record.explicit.as_ref() != Some(&wait) {
                record.explicit = Some(wait);
                record.wait_since = now;
            }
        }
    }

    fn transition(&mut self, entry: &str, phase: EntryPhase, now: Instant) {
        if let Some(record) = self.records.get_mut(entry) {
            if record.phase != phase {
                record.phase = phase;
                record.since = now;
                record.wait_since = now;
                record.explicit = None;
            }
        }
    }

    /// Marks every entry that waits for `failed` as `not started`, and theirs,
    /// so a failure never leaves a dependent claiming to run.
    fn fail_dependents(&mut self, failed: &str, attribution: Attribution, now: Instant) {
        let mut blocked = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        seen.insert(failed.to_owned());
        let mut frontier = vec![failed.to_owned()];
        while let Some(name) = frontier.pop() {
            for entry in self.order.entries() {
                if !entry.waits_for.iter().any(|dependency| dependency == &name) {
                    continue;
                }
                if !seen.insert(entry.name.clone()) {
                    continue;
                }
                blocked.push(entry.name.clone());
                frontier.push(entry.name.clone());
            }
        }
        for name in blocked {
            self.mark_not_started(&name, attribution.clone(), now);
        }
    }

    fn mark_not_started(&mut self, entry: &str, attribution: Attribution, now: Instant) {
        let Some(record) = self.records.get_mut(entry) else {
            return;
        };
        if record.phase.started() {
            // It already started: this failure is not its prerequisite.
            return;
        }
        if record.phase == EntryPhase::NotStarted {
            return;
        }
        record.phase = EntryPhase::NotStarted;
        record.since = now;
        record.wait_since = now;
        record.explicit = None;
        record.not_started_because = Some(attribution);
    }

    /// Whether the entry may start now: everything its order names is
    /// satisfied.
    pub fn may_start(&self, entry: &str) -> bool {
        let Some(record) = self.records.get(entry) else {
            return false;
        };
        if record.phase != EntryPhase::Waiting {
            return false;
        }
        self.unmet_prerequisites(entry).is_empty() && record.explicit.is_none()
    }

    /// The entries whose prerequisite failed, in resolved order.
    pub fn not_started(&self) -> Vec<&str> {
        self.order
            .names()
            .filter(|name| {
                self.records
                    .get(*name)
                    .is_some_and(|record| record.phase == EntryPhase::NotStarted)
            })
            .collect()
    }

    /// The name of the first prerequisite the entry still waits for.
    fn unmet_prerequisites(&self, entry: &str) -> Vec<String> {
        let Some(order) = self.order.get(entry) else {
            return Vec::new();
        };
        let mut unmet = Vec::new();
        if order.after_environment
            && self
                .environment
                .as_ref()
                .is_some_and(|environment| environment.state != EntryPhase::Ready)
        {
            unmet.push("environment".to_owned());
        }
        for dependency in &order.waits_for {
            let Some(record) = self.records.get(dependency) else {
                continue;
            };
            let satisfied = if order.waits_for_readiness {
                record.phase == EntryPhase::Ready
            } else {
                record.phase.started()
            };
            if !satisfied && !record.phase.blocked() && record.phase != EntryPhase::Failed {
                unmet.push(dependency.clone());
            }
        }
        if let Some(holder) = order.held_by.as_deref() {
            if let Some(record) = self.records.get(holder) {
                // The plan proceeds past the holder only once its own launch
                // and readiness observation are over.
                if !plan_moved_past(record.phase) {
                    unmet.push(holder.to_owned());
                }
            }
        }
        unmet
    }

    /// The wait an entry shows while it is `Waiting`, derived from the graph.
    fn derived_wait(&self, entry: &str) -> Option<WaitOn> {
        let order = self.order.get(entry)?;
        if order.after_environment {
            if let Some(environment) = self.environment.as_ref() {
                if environment.state != EntryPhase::Ready {
                    return Some(WaitOn::Environment {
                        healthy: environment.healthy,
                        total: environment.total,
                    });
                }
            }
        }
        for dependency in &order.waits_for {
            let Some(record) = self.records.get(dependency) else {
                continue;
            };
            let satisfied = if order.waits_for_readiness {
                record.phase == EntryPhase::Ready
            } else {
                record.phase.started()
            };
            if !satisfied && !record.phase.blocked() {
                return Some(WaitOn::Entry {
                    name: dependency.clone(),
                    phase: record.phase,
                });
            }
        }
        if let Some(holder) = order.held_by.as_deref() {
            // The scheduler held this entry while the entries before it are
            // launched one at a time: that is its own wait, never a declared
            // dependency.
            if let Some(record) = self.records.get(holder) {
                if !plan_moved_past(record.phase) {
                    return Some(WaitOn::Launch {
                        name: holder.to_owned(),
                        phase: record.phase,
                    });
                }
            }
        }
        None
    }

    /// The entry's state now.
    pub fn state(&self, entry: &str, now: Instant) -> Option<EntryState> {
        let record = self.records.get(entry)?;
        let (waiting_on, elapsed) = match record.phase {
            EntryPhase::Waiting => {
                let derived = self.derived_wait(entry);
                let wait = record.explicit.clone().or(derived);
                (wait.clone(), now.duration_since(record.wait_since))
            }
            EntryPhase::Starting => (
                record.explicit.clone(),
                now.duration_since(match record.explicit {
                    // A probe's clock starts when the probe did.
                    Some(_) => record.wait_since,
                    None => record.since,
                }),
            ),
            _ => (None, now.duration_since(record.since)),
        };
        Some(EntryState {
            entry: entry.to_owned(),
            phase: record.phase,
            waiting_on,
            not_started_because: record.not_started_because.clone(),
            detail: record.detail.clone(),
            elapsed_ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        })
    }

    /// Every entry's state, in the session's resolved order.
    pub fn states(&self, now: Instant) -> Vec<EntryState> {
        self.order
            .names()
            .filter_map(|entry| self.state(entry, now))
            .collect()
    }

    /// Keeps a derived wait's elapsed time starting where that wait started:
    /// when the wait an entry shows changes, its clock restarts, so one
    /// entry's elapsed time never appears on another wait.
    pub fn refresh(&mut self, now: Instant) {
        let names = self.order.names().map(str::to_owned).collect::<Vec<_>>();
        for name in names {
            let derived = self.derived_wait(&name);
            let Some(record) = self.records.get_mut(&name) else {
                continue;
            };
            if record.phase != EntryPhase::Waiting || record.explicit.is_some() {
                continue;
            }
            if record.last_derived.as_ref() != derived.as_ref() {
                record.last_derived = derived;
                record.wait_since = now;
            }
        }
    }
}

/// Whether the plan has finished with an entry: it observed the entry's
/// launch and readiness, or the entry can never start.
fn plan_moved_past(phase: EntryPhase) -> bool {
    matches!(
        phase,
        EntryPhase::Ready | EntryPhase::Exited | EntryPhase::Failed | EntryPhase::NotStarted
    )
}

fn seconds(elapsed: Duration) -> String {
    format!("{}s", elapsed.as_secs())
}

/// The whole seconds a delay still holds, never rounded down to zero while it
/// still holds the entry.
fn whole_seconds(remaining_ms: u64) -> u64 {
    remaining_ms.div_ceil(1000)
}

#[cfg(test)]
#[path = "waits/tests.rs"]
mod tests;
