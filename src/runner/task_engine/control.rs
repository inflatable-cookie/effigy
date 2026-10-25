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
use super::status::{project_foreground, project_run, RunView};
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
}

impl RunRecord {
    pub(in crate::runner) fn summary(&self) -> Option<&RunSummary> {
        match &self.source {
            RunSource::Journal(summary) => Some(summary),
            RunSource::Foreground(_) => None,
        }
    }
    pub(in crate::runner) fn foreground(&self) -> bool {
        matches!(self.source, RunSource::Foreground(_))
    }
    pub(in crate::runner) fn closed(&self) -> bool {
        self.summary()
            .is_some_and(|summary| summary.phase == Phase::Closed)
    }
    /// This observation says only that custody released its gate, never
    /// that the foreground workload succeeded or produced a retained result.
    pub(in crate::runner) fn foreground_released(&self) -> bool {
        let RunSource::Foreground(holder) = &self.source else {
            return false;
        };
        matches!(
            holder.released(&kernel::observe(), &self.root),
            Observed::Verified(true, _)
        )
    }
    pub(in crate::runner) fn refresh(&self) -> Lookup {
        lookup_at(&kernel::observe(), &self.root, self.id)
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

/// A run id, else a `--run-id` key. A string that is a valid run id is read
/// as one; a key is read through the id it derives.
pub(in crate::runner) fn resolve(selector: &str) -> Result<Lookup, RunnerError> {
    if let Ok(id) = selector.parse::<RunId>() {
        match lookup(id) {
            Lookup::NotFound => {}
            result => return Ok(result),
        }
    }
    match RunKey::parse(selector) {
        Ok(key) => Ok(lookup_key(&key)),
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
    let mut found = None;
    for id in namespace.ids {
        match lookup_journal(cx, root, id) {
            Lookup::Found(row) => {
                if found.is_some() {
                    return Lookup::Unknown(format!(
                        "key `{}` names multiple retained runs in {}",
                        key.key(),
                        root.owning_directory().path().display()
                    ));
                }
                found = Some(row);
            }
            Lookup::NotFound => {}
            other => return other,
        }
    }
    found.map_or(Lookup::NotFound, Lookup::Found)
}

fn lookup_key(key: &RunKey) -> Lookup {
    let cx = kernel::observe();
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => return Lookup::Unknown(error.to_string()),
    };
    let current = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
        .map(|resolved| resolved.resolved_root)
        .unwrap_or(cwd);
    match kernel::observe_root(&cx, &current) {
        Observed::Verified(root, _) => match lookup_key_at(&cx, &root, key) {
            Lookup::NotFound => {}
            result => return result,
        },
        Observed::Absent(_) => {}
        Observed::Unknown(reason) => {
            return Lookup::Unknown(format!("{}: {reason:?}", current.display()))
        }
    }
    let view = match kernel::known_roots_view(&cx) {
        Ok(view) => view,
        Err(error) => return Lookup::Unknown(error.to_string()),
    };
    let mut found: Option<Box<RunRecord>> = None;
    let mut omitted = view.omitted;
    for root in view.roots {
        match lookup_key_at(&cx, &root, key) {
            Lookup::Found(row) => {
                if let Some(previous) = found {
                    return Lookup::Unknown(format!(
                        "key `{}` is ambiguous between {} and {}; query it in its owning checkout",
                        key.key(),
                        previous.root.owning_directory().path().display(),
                        root.owning_directory().path().display()
                    ));
                }
                found = Some(row);
            }
            Lookup::NotFound => {}
            Lookup::Unknown(reason) => omitted.push(reason),
        }
    }
    if !omitted.is_empty() {
        return Lookup::Unknown(format!(
            "key discovery is incomplete: {}",
            omitted.join("; ")
        ));
    }
    found.map_or(Lookup::NotFound, Lookup::Found)
}

/// A caller already resolved the owning checkout. Absence or uncertainty
/// there never falls through to another checkout or the machine store.
pub(in crate::runner) fn lookup_checkout(checkout: &Path, id: RunId) -> Lookup {
    let cx = kernel::observe();
    match kernel::observe_root(&cx, checkout) {
        Observed::Verified(root, _) => lookup_at(&cx, &root, id),
        Observed::Absent(_) => Lookup::NotFound,
        Observed::Unknown(reason) => Lookup::Unknown(format!("{}: {reason:?}", checkout.display())),
    }
}

pub(in crate::runner) fn lookup(id: RunId) -> Lookup {
    let cx = kernel::observe();
    // Current-checkout authority wins before cross-checkout discovery. An
    // unreadable cwd journal is Unknown, never permission to select a peer.
    if let Ok(cwd) = std::env::current_dir() {
        let current = effigy_core::resolver::resolve_target_root(cwd.clone(), None)
            .map(|resolved| resolved.resolved_root)
            .unwrap_or(cwd);
        match kernel::observe_root(&cx, &current) {
            Observed::Verified(root, _) => match lookup_at(&cx, &root, id) {
                Lookup::NotFound => {}
                result => return result,
            },
            Observed::Absent(_) => {}
            Observed::Unknown(reason) => {
                return Lookup::Unknown(format!("{}: {reason:?}", current.display()))
            }
        }
    }
    let roots = match kernel::known_roots(&cx) {
        Ok(roots) => roots,
        Err(error) => return Lookup::Unknown(error.to_string()),
    };
    let mut found: Option<Box<RunRecord>> = None;
    let mut unknown = Vec::new();
    for root in roots {
        match lookup_at(&cx, &root, id) {
            Lookup::Found(record) => {
                if let Some(previous) = found {
                    return Lookup::Unknown(format!("run {id} is ambiguous between {} and {}; run the query in its owning checkout",
                        previous.root.owning_directory().path().display(), root.owning_directory().path().display()));
                }
                found = Some(record);
            }
            Lookup::NotFound => {}
            Lookup::Unknown(reason) => unknown.push(reason),
        }
    }
    if let Some(record) = found {
        Lookup::Found(record)
    } else if !unknown.is_empty() {
        Lookup::Unknown(unknown.join("; "))
    } else {
        Lookup::NotFound
    }
}

fn lookup_at(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
) -> Lookup {
    let foreground = match foreground_records(cx, root) {
        Ok(rows) => rows.into_iter().find(|row| row.id == id),
        Err(reason) => return Lookup::Unknown(reason),
    };
    let durable = lookup_journal(cx, root, id);
    match (foreground, durable) {
        (Some(_), Lookup::Found(_)) => Lookup::Unknown(format!(
            "run {id} has conflicting foreground and journal evidence"
        )),
        (Some(row), _) => Lookup::Found(Box::new(row)),
        (None, result) => result,
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
                "{}: foreground custody unavailable: {reason:?}",
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

fn lookup_journal(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    id: RunId,
) -> Lookup {
    let reader = match Store::read(cx, root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Lookup::NotFound,
        Observed::Unknown(reason) => {
            return Lookup::Unknown(format!("{}: {reason:?}", root.path().display()))
        }
    };
    match effigy_kernel::run::summary(&reader, id) {
        Observed::Verified(summary, _) => Lookup::Found(Box::new(record(cx, root, summary))),
        Observed::Absent(_) => Lookup::NotFound,
        Observed::Unknown(reason) => {
            Lookup::Unknown(format!("{}: {reason:?}", root.path().display()))
        }
    }
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
            Observed::Unknown(reason) => return Err(unreadable(&format!("{reason:?}"))),
        },
        None => {
            let view = kernel::known_roots_view(&cx)?;
            omitted = view.omitted;
            view.roots
        }
    };
    let observed_roots = roots.len();
    let mut records = Vec::new();
    for state in roots {
        match foreground_records(&cx, &state) {
            Ok(rows) => records.extend(rows.into_iter().filter(|record| match &record.source {
                RunSource::Foreground(row) => {
                    row.coordinates.owner.kind() == ResourceKind::Task || row.coordinates.root
                }
                RunSource::Journal(_) => false,
            })),
            Err(reason) => omitted.push(reason),
        }
        let reader = match Store::read(&cx, &state) {
            Observed::Verified(reader, _) => reader,
            Observed::Absent(_) => continue,
            Observed::Unknown(reason) => {
                omitted.push(format!(
                    "{}: journal unavailable: {reason:?}",
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
                    "{}: journal unavailable: {reason:?}",
                    state.path().display()
                ));
                continue;
            }
        };
        records.extend(
            rows.into_iter()
                .map(|summary| record(&cx, &state, summary))
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
    }
    records.sort_by_key(|record| {
        std::cmp::Reverse((
            record
                .summary()
                .map_or(i64::MAX, |summary| summary.created_at_ms),
            record.id.to_string(),
        ))
    });
    records.truncate(limit);
    Ok(RunListing {
        runs: records,
        omitted,
        observed_roots,
    })
}

fn root_for_run(id: RunId) -> Observed<effigy_kernel::root::StateRoot> {
    match lookup(id) {
        Lookup::Found(record) => Observed::Verified(
            record.root,
            effigy_kernel::observe::Evidence::new("bound checkout journal".into(), None)
                .expect("fixed source"),
        ),
        Lookup::NotFound => Observed::Absent(effigy_kernel::observe::AbsenceProof::NoStateRoot),
        Lookup::Unknown(reason) => Observed::Unknown(
            effigy_kernel::observe::UnknownReason::StoreUnreadable(reason),
        ),
    }
}

fn unreadable(reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!("the run journal is unreadable: {reason}"))
}

/// A run's effects as its journal records them: a pure read (R2).
pub(in crate::runner) fn effects(id: RunId) -> Observed<Vec<effigy_kernel::run::Effect>> {
    let cx = kernel::observe();
    let root = match root_for_run(id) {
        Observed::Verified(root, _) => root,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => effigy_kernel::run::effects(&reader, id),
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
}

/// The run's workload process: its last custody process that is not the
/// admission hook or an env-schema command. `None` until it has started.
pub(in crate::runner) fn workload_process(id: RunId) -> Observed<Option<i64>> {
    let cx = kernel::observe();
    let root = match root_for_run(id) {
        Observed::Verified(root, _) => root,
        Observed::Absent(proof) => return Observed::Absent(proof),
        Observed::Unknown(reason) => return Observed::Unknown(reason),
    };
    let reader = match Store::read(&cx, &root) {
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
    id: RunId,
    sequence: i64,
    stream: Stream,
    offset: u64,
    max: usize,
) -> Observed<LogRead> {
    let cx = kernel::observe();
    match root_for_run(id) {
        Observed::Verified(root, _) => {
            effigy_kernel::run::read_log(&cx, &root, id, sequence, stream, offset, max)
        }
        Observed::Absent(proof) => Observed::Absent(proof),
        Observed::Unknown(reason) => Observed::Unknown(reason),
    }
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
        Err(effigy_kernel::run::RunError::Invalid(reason))
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

/// Bound for one journal poll while following or stopping.
pub(in crate::runner) const POLL: Duration = Duration::from_millis(100);
