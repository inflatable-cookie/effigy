//! Generic run control over checkout journals and live foreground gates.
//! Durable runs retain outcomes and logs; foreground diagnostics exist only
//! while custody holds a gate. Reads are pure (R2). Foreground stop never
//! opens a journal or reconstructs signalling authority from old gate bytes.

use std::path::Path;
use std::time::Duration;

use effigy_kernel::custody::Stream;
use effigy_kernel::gate::{ForegroundHolder, Gate};
use effigy_kernel::id::{ResourceKind, RunId};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{Fence, LogRead, Phase, Request, RequestAction, RunSummary};
use effigy_kernel::store::Store;

use super::kernel::{self, TaskSettlement};
use super::status::{project_foreground, project_run, project_terminal, RunView};
use super::RunKey;
use crate::runner::error::RunnerError;

/// Process labels that are part of a run but not its workload output.
const NOT_WORKLOAD: &[&str] = &[super::admission::HOOK_LABEL, super::process::ENV_EXEC_LABEL];
const KERNEL_PROCESS: &str = "kernel:process";

/// One run as the journal and its owner gate report it now.
#[derive(Debug, Clone)]
pub(in crate::runner) struct RunRecord {
    pub(in crate::runner) id: RunId,
    /// Bound once by lookup; stop and logs retain this checkout generation.
    pub(super) root: effigy_kernel::root::StateRoot,
    source: RunSource,
    pub(in crate::runner) view: RunView,
    /// The selector the run was admitted for.
    pub(in crate::runner) task: String,
}

#[derive(Debug, Clone)]
enum RunSource {
    Journal(RunSummary),
    Foreground(ForegroundHolder),
    /// The narrow Q-010 exception: a settled v2 hook-admitted foreground
    /// run's journal row. Evidence, never a live or recoverable run.
    Terminal(effigy_kernel::run::TerminalRecord),
}

impl RunRecord {
    pub(in crate::runner) fn summary(&self) -> Option<&RunSummary> {
        match &self.source {
            RunSource::Journal(summary) => Some(summary),
            RunSource::Foreground(_) | RunSource::Terminal(_) => None,
        }
    }
    pub(in crate::runner) fn foreground(&self) -> bool {
        matches!(self.source, RunSource::Foreground(_))
    }
    /// A terminal-record run: settled, with no live owner and no recoverable
    /// process identity.
    pub(in crate::runner) fn is_terminal_record(&self) -> bool {
        matches!(self.source, RunSource::Terminal(_))
    }
    pub(in crate::runner) fn closed(&self) -> bool {
        match &self.source {
            RunSource::Terminal(_) => true,
            _ => self
                .summary()
                .is_some_and(|summary| summary.phase == Phase::Closed),
        }
    }
    /// The time this run is ordered by in `effigy runs`.
    pub(in crate::runner) fn sort_time(&self) -> i64 {
        match &self.source {
            RunSource::Journal(summary) => summary.created_at_ms,
            RunSource::Foreground(_) => i64::MAX,
            RunSource::Terminal(record) => record.closed_at_ms,
        }
    }
    /// This observation says only that custody released its gate, never
    /// that the foreground workload succeeded or produced a retained result.
    pub(in crate::runner) fn foreground_released(&self) -> bool {
        let RunSource::Foreground(holder) = &self.source else {
            return false;
        };
        matches!(
            holder.released(&kernel::observe()),
            Observed::Verified(true, _)
        )
    }
    pub(in crate::runner) fn refresh(&self) -> Lookup {
        lookup_at(&kernel::observe(), &self.root, self.id)
    }
    /// The run's workload process, read from this record's own journal: a poll
    /// pays one read here, not another lookup (effigy#339).
    pub(in crate::runner) fn workload_process(&self) -> Observed<Option<i64>> {
        workload_process_in(&kernel::observe(), &self.root, self.id)
    }
    pub(in crate::runner) fn effects(&self) -> Observed<Vec<effigy_kernel::run::Effect>> {
        match Store::read(&kernel::observe(), &self.root) {
            Observed::Verified(reader, _) => effigy_kernel::run::effects(&reader, self.id),
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
        }
    }
    pub(in crate::runner) fn read_log(
        &self,
        sequence: i64,
        stream: Stream,
        offset: u64,
        max: usize,
    ) -> Observed<LogRead> {
        effigy_kernel::run::read_log(
            &kernel::observe(),
            &self.root,
            self.id,
            sequence,
            stream,
            offset,
            max,
        )
    }

    /// Closed, or open with a free owner gate: either way it will make no
    /// more progress by itself.
    pub(in crate::runner) fn terminal(&self) -> bool {
        matches!(self.view.state.as_str(), "interrupted") || self.closed()
    }

    /// The run's own result as a process exit code.
    pub(in crate::runner) fn exit_code(&self) -> i32 {
        match self.view.exit_code {
            Some(code) if self.closed() => code,
            _ => 1,
        }
    }
}

#[derive(Debug)]
pub(in crate::runner) enum Lookup {
    Found(Box<RunRecord>),
    NotFound,
    /// The journal could not be observed (R3).
    Unknown(String),
}

/// A run id, else a `--run-id` key, in `checkout`. A string that is a valid
/// run id is read as one; a key is read through the id it derives.
pub(in crate::runner) fn resolve_in(
    checkout: &Path,
    selector: &str,
) -> Result<Lookup, RunnerError> {
    if let Ok(id) = selector.parse::<RunId>() {
        match lookup_checkout(checkout, id) {
            Lookup::NotFound => {}
            result => return Ok(result),
        }
    }
    match RunKey::parse(selector) {
        Ok(key) => Ok(lookup_key_in(checkout, &key)),
        Err(_) => selector
            .parse::<RunId>()
            .map(|_| Lookup::NotFound)
            .map_err(|_| {
                RunnerError::task_invocation(format!(
                    "`{selector}` is neither a run id nor a run key"
                ))
            }),
    }
}

fn lookup_key_at(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    key: &RunKey,
) -> Lookup {
    let namespace = match key.namespace(cx, root) {
        Ok(namespace) => namespace,
        Err(error) => return Lookup::Unknown(error.to_string()),
    };
    // One read answers every id the key derives: a poll is one cheap read.
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Lookup::NotFound,
        Observed::Unknown(reason) => return Lookup::Unknown(journal_unknown(root, &reason)),
    };
    let mut found = None;
    for id in namespace.ids {
        match effigy_kernel::run::summary(&reader, id) {
            Observed::Verified(summary, _) => {
                if found.is_some() {
                    return Lookup::Unknown(format!(
                        "key `{}` names multiple retained runs in {}",
                        key.key(),
                        root.owning_directory().path().display()
                    ));
                }
                found = Some(record(cx, root, summary));
            }
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => return Lookup::Unknown(journal_unknown(root, &reason)),
        }
    }
    found.map_or(Lookup::NotFound, |row| Lookup::Found(Box::new(row)))
}

/// A journal read that could not answer, named by its checkout.
fn journal_unknown(
    root: &effigy_kernel::root::StateRoot,
    reason: &effigy_kernel::observe::UnknownReason,
) -> String {
    format!("{}: {reason}", root.path().display())
}

/// A key's run in one checkout. Peer worktrees are never scanned: a lookup
/// that read every sibling journal held a poll at 90% CPU for 75 minutes
/// (effigy#339, Q-011). `runs --all` is the cross-checkout view.
fn lookup_key_in(checkout: &Path, key: &RunKey) -> Lookup {
    let cx = kernel::observe();
    match kernel::observe_root(&cx, checkout) {
        Observed::Verified(root, _) => lookup_key_at(&cx, &root, key),
        Observed::Absent(_) => Lookup::NotFound,
        Observed::Unknown(reason) => Lookup::Unknown(format!("{}: {reason}", checkout.display())),
    }
}

/// A caller already resolved the owning checkout. Absence or uncertainty
/// there never falls through to another checkout or the machine store.
pub(in crate::runner) fn lookup_checkout(checkout: &Path, id: RunId) -> Lookup {
    let cx = kernel::observe();
    match kernel::observe_root(&cx, checkout) {
        Observed::Verified(root, _) => lookup_at(&cx, &root, id),
        Observed::Absent(_) => Lookup::NotFound,
        Observed::Unknown(reason) => Lookup::Unknown(format!("{}: {reason}", checkout.display())),
    }
}

/// One run in one journal: one `Store::read`, then the run's row and, when
/// there is none, its terminal record from that same reader.
fn lookup_at(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
) -> Lookup {
    let foreground = match foreground_records(cx, root) {
        Ok(rows) => rows.into_iter().find(|row| row.id == id),
        Err(reason) => return Lookup::Unknown(reason),
    };
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => {
            return match foreground {
                Some(row) => Lookup::Found(Box::new(row)),
                None => Lookup::NotFound,
            }
        }
        Observed::Unknown(reason) => {
            return match foreground {
                Some(row) => Lookup::Found(Box::new(row)),
                None => Lookup::Unknown(journal_unknown(root, &reason)),
            }
        }
    };
    let durable = match effigy_kernel::run::summary(&reader, id) {
        Observed::Verified(summary, _) => Some(record(cx, root, summary)),
        Observed::Absent(_) => None,
        Observed::Unknown(reason) => return Lookup::Unknown(journal_unknown(root, &reason)),
    };
    match (foreground, durable) {
        (Some(_), Some(_)) => Lookup::Unknown(format!(
            "run {id} has conflicting foreground and journal evidence"
        )),
        (Some(row), None) => Lookup::Found(Box::new(row)),
        (None, Some(record)) => Lookup::Found(Box::new(record)),
        // A v2 hook-admitted foreground run's narrow terminal row lives in
        // the same journal; it is the only post-exit evidence Q-010 keeps.
        (None, None) => match effigy_kernel::run::terminal_record(&reader, id) {
            Observed::Verified(Some(record), _) => Lookup::Found(Box::new(RunRecord {
                id,
                root: root.clone(),
                view: project_terminal(&record),
                task: record.task.clone(),
                source: RunSource::Terminal(record),
            })),
            Observed::Verified(None, _) | Observed::Absent(_) => Lookup::NotFound,
            Observed::Unknown(reason) => Lookup::Unknown(journal_unknown(root, &reason)),
        },
    }
}

fn foreground_records(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
) -> Result<Vec<RunRecord>, String> {
    let rows = match Gate::foreground_holders(cx, root) {
        Observed::Verified(rows, _) => rows,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => {
            return Err(format!(
                "{}: foreground custody unavailable: {reason}",
                root.path().display()
            ))
        }
    };
    let mut unique = std::collections::BTreeMap::new();
    for row in rows {
        let Some(id) = row
            .holder
            .run
            .as_ref()
            .and_then(|run| run.parse::<RunId>().ok())
        else {
            return Err("foreground holder has no readable run id".into());
        };
        if let Some(previous) = unique.get(&id.to_string()) {
            let previous: &RunRecord = previous;
            if let RunSource::Foreground(old) = &previous.source {
                if old.coordinates.owner != row.coordinates.owner
                    || old.holder.pid != row.holder.pid
                    || old.holder.start != row.holder.start
                    || old.holder.boot != row.holder.boot
                {
                    return Err(format!("run {id} has conflicting foreground holders"));
                }
            }
            continue;
        }
        unique.insert(
            id.to_string(),
            RunRecord {
                id,
                root: root.clone(),
                view: project_foreground(&row, id.to_string()),
                task: row.coordinates.task.task().to_owned(),
                source: RunSource::Foreground(row),
            },
        );
    }
    Ok(unique.into_values().collect())
}

fn record(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    summary: RunSummary,
) -> RunRecord {
    let gate = match (&summary.invocation, summary.phase) {
        (_, Phase::Closed) => None,
        (Some(invocation), _) => Some(Gate::probe(cx, root, &invocation.spec().owner)),
        (None, _) => Some(Observed::Unknown(
            effigy_kernel::observe::UnknownReason::Inconsistent(
                "run invocation is unreadable".to_owned(),
            ),
        )),
    };
    let view = project_run(&summary, gate.as_ref());
    let task = summary
        .invocation
        .as_ref()
        .and_then(|invocation| invocation.spec().argv.first())
        .map(|label| label.to_string_lossy().into_owned())
        .unwrap_or_default();
    RunRecord {
        id: summary.id,
        root: root.clone(),
        source: RunSource::Journal(summary),
        view,
        task,
    }
}

/// The newest runs of tasks and QA groups, in the checkout at `root`, or on
/// the whole host when `root` is `None`. Internal lock sections are hidden.
#[derive(Debug)]
pub(in crate::runner) struct RunListing {
    pub runs: Vec<RunRecord>,
    pub omitted: Vec<String>,
    pub observed_roots: usize,
}

pub(in crate::runner) fn list(
    root: Option<&Path>,
    limit: usize,
) -> Result<RunListing, RunnerError> {
    let cx = kernel::observe();
    let mut omitted = Vec::new();
    let roots = match root {
        Some(path) => match kernel::observe_root(&cx, path) {
            Observed::Verified(root, _) => vec![root],
            Observed::Absent(_) => Vec::new(),
            Observed::Unknown(reason) => return Err(unreadable(&format!("{reason}"))),
        },
        None => {
            let view = kernel::known_roots_view(&cx)?;
            omitted = view.omitted;
            view.roots
        }
    };
    list_roots(&cx, roots, omitted, limit)
}

/// Observe this already-owned checkout with the caller's read budget.
pub(in crate::runner) fn list_owned(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    limit: usize,
) -> Result<RunListing, RunnerError> {
    cx.check("workspace run listing")
        .map_err(|error| unreadable(&error.to_string()))?;
    let result = list_roots(cx, vec![root.clone()], Vec::new(), limit)?;
    cx.check("workspace run listing")
        .map_err(|error| unreadable(&error.to_string()))?;
    Ok(result)
}

fn list_roots(
    cx: &effigy_kernel::ctx::ObserveContext,
    roots: Vec<effigy_kernel::root::StateRoot>,
    mut omitted: Vec<String>,
    limit: usize,
) -> Result<RunListing, RunnerError> {
    let observed_roots = roots.len();
    let mut records = Vec::new();
    for state in roots {
        match foreground_records(cx, &state) {
            Ok(rows) => records.extend(rows.into_iter().filter(|record| match &record.source {
                RunSource::Foreground(row) => {
                    row.coordinates.owner.kind() == ResourceKind::Task || row.coordinates.root
                }
                RunSource::Journal(_) | RunSource::Terminal(_) => false,
            })),
            Err(reason) => omitted.push(reason),
        }
        let reader = match Store::read(cx, &state) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(_) => continue,
            Observed::Unknown(reason) => {
                omitted.push(format!(
                    "{}: journal unavailable: {reason}",
                    state.path().display()
                ));
                continue;
            }
        };
        let rows = match effigy_kernel::run::recent(&reader, None, limit.saturating_mul(4).max(64))
        {
            Observed::Verified(rows, _) => rows,
            Observed::Absent(_) => Vec::new(),
            Observed::Unknown(reason) => {
                omitted.push(format!(
                    "{}: journal unavailable: {reason}",
                    state.path().display()
                ));
                continue;
            }
        };
        records.extend(
            rows.into_iter()
                .map(|summary| record(cx, &state, summary))
                .filter(|record| {
                    record
                        .summary()
                        .and_then(|summary| summary.invocation.as_ref())
                        .is_some_and(|invocation| {
                            invocation.spec().owner.kind() == ResourceKind::Task
                        })
                        || record.task.starts_with("qa-group:")
                }),
        );
        match effigy_kernel::run::terminal_records(&reader, limit.max(64)) {
            Observed::Verified(terminals, _) => {
                records.extend(terminals.into_iter().map(|terminal| {
                    RunRecord {
                        id: terminal
                            .run_id
                            .parse::<RunId>()
                            .unwrap_or_else(|_| RunId::new(0, [0; 10]).expect("fixed run id")),
                        root: state.clone(),
                        view: project_terminal(&terminal),
                        task: terminal.task.clone(),
                        source: RunSource::Terminal(terminal),
                    }
                }));
            }
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => omitted.push(format!(
                "{}: foreground terminal records unavailable: {reason}",
                state.path().display()
            )),
        }
    }
    records.sort_by_key(|record| std::cmp::Reverse((record.sort_time(), record.id.to_string())));
    records.truncate(limit);
    Ok(RunListing {
        runs: records,
        omitted,
        observed_roots,
    })
}

fn unreadable(reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!("the run journal is unreadable: {reason}"))
}

/// A run's effects from one checkout's journal: a pure read (R2). A follower
/// binds the root once and keeps reading it, so a poll never re-resolves the
/// run (effigy#339).
pub(in crate::runner) fn effects_in(
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
) -> Observed<Vec<effigy_kernel::run::Effect>> {
    match Store::read(&kernel::observe(), root) {
        Observed::Verified(reader, _) => effigy_kernel::run::effects(&reader, id),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// The run's workload process: its last custody process that is not the
/// admission hook or an env-schema command. `None` until it has started.
fn workload_process_in(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
) -> Observed<Option<i64>> {
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    match effigy_kernel::run::effects(&reader, id) {
        Observed::Verified(effects, evidence) => Observed::Verified(
            effects
                .iter()
                .filter(|effect| effect.spec.kind == KERNEL_PROCESS)
                .filter(|effect| {
                    let label = effect.spec.payload.get("label").and_then(|v| v.as_str());
                    !label.is_some_and(|label| NOT_WORKLOAD.contains(&label))
                })
                .map(|effect| effect.sequence)
                .next_back(),
            evidence,
        ),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// Reads one stream of the run's workload process from an exact offset.
pub(in crate::runner) fn read_log(
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
    sequence: i64,
    stream: Stream,
    offset: u64,
    max: usize,
) -> Observed<LogRead> {
    effigy_kernel::run::read_log(&kernel::observe(), root, id, sequence, stream, offset, max)
}

/// What `stop` did to reach the run's owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum StopStarted {
    /// The run was already closed.
    AlreadyClosed,
    /// A fenced stop request is in the journal for the live owner.
    Requested,
    /// The owner was gone; the run was settled here, by the gate's holder.
    Settled,
}

fn effects_for_record(record: &RunRecord) -> Result<effigy_kernel::root::StateRoot, RunnerError> {
    let root = if record.root.is_machine() {
        let home = kernel::state_home()
            .ok_or_else(|| RunnerError::task_invocation("HOME is unset or relative"))?;
        effigy_kernel::root::StateRoot::runtime_machine_effects(&kernel::effects(), &home)
            .map_err(|error| RunnerError::task_invocation(error.to_string()))?
    } else {
        kernel::effects_root(record.root.owning_directory().path())?.clone()
    };
    let identity = |dir: &effigy_kernel::fs::DirFd| {
        dir.identity()
            .map_err(|error| RunnerError::task_invocation(error.to_string()))
    };
    if identity(root.owning_directory())? != identity(record.root.owning_directory())?
        || identity(root.state_directory())? != identity(record.root.state_directory())?
    {
        return Err(RunnerError::task_invocation(
            "bound run checkout was replaced before the action",
        ));
    }
    Ok(root)
}

/// The kernel stop path: a generation-fenced `Stop` request the live owner
/// acts on, or, when the owner is gone, the one settle under its gate.
pub(in crate::runner) fn request_stop(record: &RunRecord) -> Result<StopStarted, RunnerError> {
    let summary = record.summary().ok_or_else(|| {
        RunnerError::task_invocation("foreground runs accept only a verified live-owner signal")
    })?;
    if record.closed() {
        return Ok(StopStarted::AlreadyClosed);
    }
    let invocation = summary.invocation.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "run {} has no readable invocation; it cannot be addressed",
            record.id
        ))
    })?;
    let owner = invocation.spec().owner.clone();
    let root = effects_for_record(record)?;
    let cx = kernel::effects();
    if record.view.state == "interrupted" {
        let cleanup = kernel::cleanup()?;
        let next = kernel::next_invocation(&record.task)?;
        return effigy_kernel::run::settle_interrupted(
            &cx,
            &root,
            &owner,
            record.id,
            next,
            &cleanup,
            &mut TaskSettlement::default(),
        )
        .map(|_| StopStarted::Settled)
        .map_err(|error| {
            RunnerError::task_invocation(format!("run {} could not be settled: {error}", record.id))
        });
    }
    let request = Request {
        id: super::run::fresh_run_id()?,
        run: record.id,
        expected: Fence {
            resource: owner,
            generation: summary.generation,
        },
        action: RequestAction::Stop,
    };
    match effigy_kernel::run::request(&cx, &root, &request) {
        Ok(()) => Ok(StopStarted::Requested),
        // The run settled between this record's snapshot and the request: a
        // concurrent stop, owner death or recovery already closed the exact
        // run this command addressed. That is the idempotent outcome, not a
        // refusal (the supervisor fault matrix crosses this boundary at every
        // journal commit).
        Err(effigy_kernel::run::RunError::Refused(reason))
            if reason == "request run is not current" =>
        {
            Ok(StopStarted::AlreadyClosed)
        }
        Err(error) => Err(RunnerError::task_invocation(format!(
            "run {} did not accept a stop request: {error}",
            record.id
        ))),
    }
}

/// For an owner that does not read stop requests (a sequence, managed or
/// container run): TERM to the gate holder named by the owner gate, whose
/// start identity the gate probe verified. Its signal router forwards the
/// stop to its own process groups. Never a pattern kill.
pub(in crate::runner) fn signal_owner(record: &RunRecord) -> bool {
    if let RunSource::Foreground(holder) = &record.source {
        // Revalidate the checkout generation and holder before signalling.
        // The gate operation checks the run, native start and boot again.
        let Lookup::Found(now) = record.refresh() else {
            return false;
        };
        if !now.foreground() || now.view.state != "running" {
            return false;
        }
        return matches!(
            Gate::signal_holder(
                &kernel::effects(),
                &record.root,
                &holder.coordinates.gate,
                &record.id,
                libc::SIGTERM
            ),
            Observed::Verified(..)
        );
    }
    let Some(invocation) = record
        .summary()
        .and_then(|summary| summary.invocation.as_ref())
    else {
        return false;
    };
    let Ok(root) = effects_for_record(record) else {
        return false;
    };
    matches!(
        Gate::signal_holder(
            &kernel::effects(),
            &root,
            &invocation.spec().owner,
            &record.id,
            libc::SIGTERM,
        ),
        Observed::Verified(..)
    )
}

/// Bound for one journal poll while following or stopping: one cheap read of
/// the run's own journal per interval, never a tight spin.
pub(in crate::runner) const POLL: Duration = Duration::from_millis(250);
