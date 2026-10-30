//! Task status from transient foreground gates and durable `runs` rows
//! (contract 052 §5.6 status). There is no heartbeat and no staleness: a
//! held gate is a live run, a free gate under a non-closed row is an
//! interrupted run, and history never stands in for current state.

use effigy_kernel::gate::{ForegroundHolder, Gate, GateState};
use effigy_kernel::id::{CheckoutId, ResourceId, Scope};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::{history, recent, Phase, RunSummary};
use effigy_kernel::store::Store;
use serde::Serialize;

use super::identity::TaskTarget;
use super::kernel;

/// Runs kept per task by retention, and so the longest history shown.
pub(in crate::runner) const HISTORY_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct RunView {
    pub(in crate::runner) evidence_source: &'static str,
    pub(in crate::runner) run_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) step: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(in crate::runner) steps: Vec<RunView>,
    pub(in crate::runner) state: String,
    pub(in crate::runner) phase: Option<&'static str>,
    pub(in crate::runner) profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) signal: Option<i32>,
    /// `complete`, `incomplete`, or absent while the run is open.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) cleanup: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) pid: Option<u32>,
    pub(in crate::runner) started_at_epoch_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) finished_at_epoch_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) duration_ms: Option<i64>,
    /// Present on a foreground terminal record: the checkout whose journal
    /// holds it, and the boot the run's custodians lived in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) checkout_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) boot: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct TaskStatusView {
    /// `running`, `stopping`, `interrupted`, a recorded residue, `not-recorded`
    /// or `unknown`.
    pub(in crate::runner) state: String,
    /// The newest run while it is open (live or interrupted).
    pub(in crate::runner) current: Option<RunView>,
    /// The newest closed run.
    pub(in crate::runner) latest: Option<RunView>,
    pub(in crate::runner) history: Vec<RunView>,
    /// The command that resolves an interrupted run.
    pub(in crate::runner) next: Option<String>,
    pub(in crate::runner) warnings: Vec<String>,
}

/// Foreground evidence and durable history are independent read models.
/// A historical result never claims to be the most recent invocation.
pub(in crate::runner) fn task_status(target: &TaskTarget, limit: usize) -> TaskStatusView {
    let mut view = durable_status(target, limit);
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx, &target.root) {
        Observed::Verified(root, _) => root,
        _ => return view,
    };
    let rows = match Gate::foreground_holders(&cx, &root) {
        Observed::Verified(rows, _) => rows,
        Observed::Unknown(reason) => {
            view.state = "unknown".into();
            view.warnings
                .push(format!("foreground custody unavailable: {reason:?}"));
            return view;
        }
        Observed::Absent(_) => return view,
    };
    let mut live = std::collections::BTreeMap::new();
    for row in rows {
        if row.coordinates.owner.kind() != effigy_kernel::id::ResourceKind::Task
            || row.coordinates.task.catalog() != target.catalog
            || row.coordinates.task.task() != target.task
        {
            continue;
        }
        let Some(run) = row.holder.run.clone() else {
            continue;
        };
        live.entry(run).or_insert(row);
    }
    if live.len() > 1 {
        view.state = "unknown".into();
        view.warnings
            .push("conflicting foreground gate bindings".into());
    } else if let Some((run, row)) = live.into_iter().next() {
        let verified = matches!(
            row.holder.state,
            Observed::Verified(
                effigy_kernel::ident::ProcessState::Active
                    | effigy_kernel::ident::ProcessState::Stopped,
                _
            )
        );
        view.state = if verified { "running" } else { "unknown" }.into();
        if !verified {
            view.warnings.push("foreground gate remains held but its invoking owner is unavailable; cleanup or residue cannot be reconstructed".into());
        }
        view.current = Some(project_foreground(&row, run));
        view.next = None;
    }
    view
}

pub(super) fn project_foreground(row: &ForegroundHolder, run: String) -> RunView {
    let live = matches!(
        row.holder.state,
        Observed::Verified(
            effigy_kernel::ident::ProcessState::Active
                | effigy_kernel::ident::ProcessState::Stopped,
            _
        )
    );
    RunView {
        evidence_source: "live-foreground",
        run_id: run,
        step: None,
        steps: Vec::new(),
        state: if live { "running" } else { "unknown" }.into(),
        phase: None,
        profile: None,
        exit_code: None,
        signal: None,
        cleanup: None,
        pid: live.then_some(row.holder.pid),
        started_at_epoch_ms: None,
        finished_at_epoch_ms: None,
        duration_ms: None,
        checkout_id: None,
        boot: None,
    }
}

/// The narrow Q-010 exception's view: one settled v2 hook-admitted
/// foreground run, read from its checkout journal. It is evidence only —
/// there is no process identity here, so nothing can be signalled from it.
pub(in crate::runner) fn project_terminal(record: &effigy_kernel::run::TerminalRecord) -> RunView {
    RunView {
        evidence_source: effigy_kernel::run::TERMINAL_RECORD_EVIDENCE,
        run_id: record.run_id.clone(),
        step: None,
        steps: Vec::new(),
        state: record.outcome.clone(),
        phase: Some("closed"),
        profile: None,
        exit_code: record.exit_code,
        signal: record.signal,
        cleanup: Some(match record.cleanup.as_str() {
            "complete" => "complete",
            "incomplete" => "incomplete",
            _ => "unknown",
        }),
        pid: None,
        started_at_epoch_ms: None,
        finished_at_epoch_ms: Some(record.closed_at_ms),
        duration_ms: None,
        checkout_id: Some(record.checkout.clone()),
        boot: Some(record.boot.clone()),
    }
}

/// Pure read (R2): opens nothing for writing and mints no checkout token.
fn durable_status(target: &TaskTarget, limit: usize) -> TaskStatusView {
    let mut view = TaskStatusView {
        state: "not-recorded".to_owned(),
        current: None,
        latest: None,
        history: Vec::new(),
        next: None,
        warnings: Vec::new(),
    };
    let checkout = match super::checkout::read(&target.root) {
        Ok(Some(checkout)) => checkout,
        Ok(None) => return view,
        Err(error) => {
            view.state = "unknown".to_owned();
            view.warnings
                .push(format!("checkout identity is unreadable: {error}"));
            return view;
        }
    };
    let owner = match target.owner(checkout) {
        Ok(owner) => owner,
        Err(error) => {
            view.state = "unknown".to_owned();
            view.warnings.push(error.to_string());
            return view;
        }
    };
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx, &target.root) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return view,
        Observed::Unknown(reason) => {
            view.state = "unknown".to_owned();
            view.warnings
                .push(format!("state root is unreadable: {reason:?}"));
            return view;
        }
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return view,
        Observed::Unknown(reason) => {
            view.state = "unknown".to_owned();
            view.warnings
                .push(format!("run journal is unreadable: {reason:?}"));
            return view;
        }
    };
    let runs = match task_runs(&reader, checkout, &owner, target, limit) {
        Observed::Verified(runs, _) => runs,
        Observed::Absent(_) => Vec::new(),
        Observed::Unknown(reason) => {
            view.state = "unknown".to_owned();
            view.warnings
                .push(format!("run journal is unreadable: {reason:?}"));
            return view;
        }
    };
    // Only the newest open row of a key can hold that key's gate now, so any
    // older open row of the same key is an interrupted predecessor the next
    // admission settles. An argument-scoped task (Q-015) has one key per
    // argument set, so several of its rows — different keys — are live
    // together, and each one is probed on its own gate.
    let mut live_keys = std::collections::BTreeSet::new();
    for (index, summary) in runs.iter().enumerate() {
        let row_owner = run_owner(summary, &owner);
        let key_is_live =
            summary.phase != Phase::Closed && live_keys.insert(row_owner.name().to_string());
        let held = key_is_live.then(|| Gate::probe(&cx, &root, row_owner));
        let mut run = project(summary, held.as_ref());
        match project_children(&cx, &root, &reader, summary.id, 0) {
            Ok(steps) => run.steps = steps,
            Err(reason) => {
                run.state = "unknown".to_owned();
                view.warnings.push(reason);
            }
        }
        if index == 0 && summary.phase != Phase::Closed {
            view.state = run.state.clone();
            if run.state == "interrupted" {
                view.next = Some(rerun_command(target, summary));
            }
            view.current = Some(run.clone());
        }
        if summary.phase == Phase::Closed
            && summary.cleanup_complete == Some(false)
            && !summary.acknowledged
        {
            view.state = "failed-cleanup".into();
            view.current = Some(run.clone());
            view.next = Some(rerun_command(target, summary));
        }
        if view.latest.is_none() && summary.phase == Phase::Closed {
            view.latest = Some(run.clone());
        }
        view.history.push(run);
    }
    view
}

/// A target's runs, newest first. One declared task normally has one owner
/// key. A task that opts into an argument-scoped lock (Q-015) gives each of
/// its runs the key of that run's own arguments, so its runs are found by
/// the task coordinates every one of them still records.
fn task_runs(
    reader: &effigy_kernel::store::StoreReader,
    checkout: CheckoutId,
    owner: &ResourceId,
    target: &TaskTarget,
    limit: usize,
) -> Observed<Vec<RunSummary>> {
    let limit = limit.max(1);
    if !target.is_argument_scoped() {
        return history(reader, owner, limit);
    }
    // Retention keeps `HISTORY_LIMIT` runs per key, so a window wide enough
    // for the shown limit still covers this task's rows beside its peers'.
    let window = limit.saturating_mul(4).max(64);
    match recent(reader, Some(Scope::Checkout(checkout)), window) {
        Observed::Verified(runs, evidence) => Observed::Verified(
            runs.into_iter()
                .filter(|run| {
                    run.invocation
                        .as_ref()
                        .is_some_and(|invocation| same_task(invocation.spec(), target))
                })
                .take(limit)
                .collect(),
            evidence,
        ),
        other => other,
    }
}

fn same_task(spec: &effigy_kernel::id::InvocationSpec, target: &TaskTarget) -> bool {
    spec.task.catalog() == target.catalog && spec.task.task() == target.task
}

/// The gate a recorded run holds. An imported row without an invocation
/// falls back to the queried owner.
fn run_owner<'a>(summary: &'a RunSummary, owner: &'a ResourceId) -> &'a ResourceId {
    summary
        .invocation
        .as_ref()
        .map_or(owner, |invocation| &invocation.spec().owner)
}

/// The command that settles this target's interrupted run. A run keyed by
/// its own arguments is settled by the same invocation, never by the bare
/// selector (Q-015).
fn rerun_command(target: &TaskTarget, summary: &RunSummary) -> String {
    let Some(spec) = summary
        .invocation
        .as_ref()
        .map(|invocation| invocation.spec())
    else {
        return format!("effigy {}", target.label);
    };
    if !target.is_argument_scoped() || spec.argv.len() < 2 {
        return format!("effigy {}", target.label);
    }
    let args = spec.argv[1..]
        .iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<String>>();
    format!(
        "effigy {} {}",
        target.label,
        crate::runner::util::render_passthrough_args(&args)
    )
}

/// One run's view from its row and, while it is open, its owner gate.
pub(in crate::runner) fn project_run(
    summary: &RunSummary,
    gate: Option<&Observed<GateState>>,
) -> RunView {
    project(summary, gate)
}

fn project(summary: &RunSummary, gate: Option<&Observed<GateState>>) -> RunView {
    let profile = summary
        .invocation
        .as_ref()
        .and_then(|invocation| invocation.spec().task.profile().map(str::to_owned));
    let (state, pid) = if summary.phase == Phase::Closed {
        (closed_state(summary), None)
    } else {
        match gate {
            Some(Observed::Verified(GateState::Held(diag), _)) => (
                if summary.phase == Phase::Stopping {
                    "stopping".to_owned()
                } else {
                    "running".to_owned()
                },
                diag.verified().map(|diag| diag.pid),
            ),
            Some(Observed::Unknown(_)) => ("unknown".to_owned(), None),
            _ => ("interrupted".to_owned(), None),
        }
    };
    let (exit_code, signal) = match &summary.workload {
        Some(WorkloadOutcome::Failed { code }) => (Some(*code), None),
        Some(WorkloadOutcome::Succeeded) => (Some(0), None),
        Some(WorkloadOutcome::Cancelled { signal, .. }) => {
            (signal.map(|signal| 128 + signal), *signal)
        }
        Some(WorkloadOutcome::TimedOut { .. }) => (Some(124), None),
        _ => (None, None),
    };
    RunView {
        evidence_source: "checkout-journal",
        run_id: summary.id.to_string(),
        step: summary
            .parent
            .and(summary.invocation.as_ref())
            .and_then(|inv| inv.spec().argv.first())
            .map(|label| label.to_string_lossy().into_owned()),
        steps: Vec::new(),
        state,
        phase: Some(phase_name(summary.phase)),
        profile,
        exit_code: (summary.phase == Phase::Closed)
            .then_some(exit_code)
            .flatten(),
        signal,
        cleanup: summary
            .cleanup_complete
            .map(|complete| if complete { "complete" } else { "incomplete" }),
        pid,
        started_at_epoch_ms: Some(summary.created_at_ms),
        finished_at_epoch_ms: summary.closed_at_ms,
        duration_ms: summary
            .closed_at_ms
            .map(|closed| closed.saturating_sub(summary.created_at_ms)),
        checkout_id: None,
        boot: None,
    }
}

fn closed_state(summary: &RunSummary) -> String {
    if summary.cleanup_complete == Some(false) {
        return "failed-cleanup".to_owned();
    }
    match &summary.workload {
        Some(WorkloadOutcome::Succeeded) => "succeeded",
        Some(WorkloadOutcome::Failed { .. }) => "failed",
        Some(WorkloadOutcome::Cancelled { .. }) => "cancelled",
        Some(WorkloadOutcome::TimedOut { .. }) => "timed-out",
        Some(WorkloadOutcome::NotStarted) => "not-started",
        Some(WorkloadOutcome::Interrupted) => "interrupted",
        Some(WorkloadOutcome::Unknown(_)) | None => "unknown",
    }
    .to_owned()
}

fn phase_name(phase: Phase) -> &'static str {
    match phase {
        Phase::Admitted => "admitted",
        Phase::Starting => "starting",
        Phase::Active => "active",
        Phase::Stopping => "stopping",
        Phase::Closed => "closed",
    }
}

/// What the journal and the owner gate say about one run, by its id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::runner) enum RunLiveness {
    Live,
    /// Its owner exited without closing it.
    Interrupted,
    Closed {
        succeeded: bool,
    },
    NotFound,
    Unknown(String),
}

/// Pure read of one run's liveness (R2).
pub(in crate::runner) fn run_liveness(
    checkout: &std::path::Path,
    run_id: &str,
    next: &[&str],
) -> RunLiveness {
    let Ok(id) = run_id.parse::<effigy_kernel::id::RunId>() else {
        return RunLiveness::NotFound;
    };
    let Ok(next) = effigy_kernel::outcome::Invocation::validated(
        next.iter().map(std::ffi::OsString::from).collect(),
        super::kernel::valid_next,
    ) else {
        return RunLiveness::Unknown("invalid recovery command".to_owned());
    };
    let cx = kernel::observe();
    let root = match super::control::lookup_checkout(checkout, id) {
        super::control::Lookup::Found(record) => record.root,
        super::control::Lookup::NotFound => return RunLiveness::NotFound,
        super::control::Lookup::Unknown(reason) => return RunLiveness::Unknown(reason),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return RunLiveness::NotFound,
        Observed::Unknown(reason) => return RunLiveness::Unknown(format!("{reason:?}")),
    };
    match effigy_kernel::run::status(&cx, &root, &reader, id, next) {
        Observed::Verified(effigy_kernel::run::RunStatus::Live(_), _) => RunLiveness::Live,
        Observed::Verified(effigy_kernel::run::RunStatus::Interrupted { .. }, _) => {
            RunLiveness::Interrupted
        }
        Observed::Verified(
            effigy_kernel::run::RunStatus::Closed {
                workload, complete, ..
            },
            _,
        ) => RunLiveness::Closed {
            succeeded: complete && workload == WorkloadOutcome::Succeeded,
        },
        Observed::Absent(_) => RunLiveness::NotFound,
        Observed::Unknown(reason) => {
            if format!("{reason:?}").contains("QueryReturnedNoRows") {
                RunLiveness::NotFound
            } else {
                RunLiveness::Unknown(format!("{reason:?}"))
            }
        }
    }
}

fn project_children(
    cx: &effigy_kernel::ctx::ObserveContext,
    root: &effigy_kernel::root::StateRoot,
    reader: &effigy_kernel::store::StoreReader,
    parent: effigy_kernel::id::RunId,
    depth: usize,
) -> Result<Vec<RunView>, String> {
    if depth >= 12 {
        return Err("step journal exceeds the status nesting limit".to_owned());
    }
    let children = match effigy_kernel::run::children(reader, parent) {
        Observed::Verified(children, _) => children,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => {
            return Err(format!("step journal is unreadable: {reason:?}"));
        }
    };
    children
        .iter()
        .map(|summary| {
            let gate = summary
                .invocation
                .as_ref()
                .map(|inv| Gate::probe(cx, root, &inv.spec().owner));
            let mut view = project(summary, gate.as_ref());
            view.steps = project_children(cx, root, reader, summary.id, depth + 1)?;
            Ok(view)
        })
        .collect()
}
