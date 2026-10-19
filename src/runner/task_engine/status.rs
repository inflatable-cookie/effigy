//! Task status as a projection of `runs` rows plus a fresh owner-gate probe
//! (contract 052 §5.6 status). There is no heartbeat and no staleness: a
//! held gate is a live run, a free gate under a non-closed row is an
//! interrupted run, and history never stands in for current state.

use effigy_kernel::gate::{Gate, GateState};
use effigy_kernel::observe::Observed;
use effigy_kernel::outcome::WorkloadOutcome;
use effigy_kernel::run::{history, Phase, RunSummary};
use effigy_kernel::store::Store;
use serde::Serialize;

use super::identity::TaskTarget;
use super::kernel;

/// Runs kept per task by retention, and so the longest history shown.
pub(in crate::runner) const HISTORY_LIMIT: usize = 20;

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct RunView {
    pub(in crate::runner) run_id: String,
    pub(in crate::runner) state: String,
    pub(in crate::runner) phase: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
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
    pub(in crate::runner) started_at_epoch_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) finished_at_epoch_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(in crate::runner) duration_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub(in crate::runner) struct TaskStatusView {
    /// `running`, `stopping`, `interrupted`, a closed outcome, `never-run`
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

impl TaskStatusView {
    pub(in crate::runner) fn has_runs(&self) -> bool {
        !self.history.is_empty()
    }
}

/// Pure read (R2): opens nothing for writing and mints no checkout token.
pub(in crate::runner) fn task_status(target: &TaskTarget, limit: usize) -> TaskStatusView {
    let mut view = TaskStatusView {
        state: "never-run".to_owned(),
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
    let root = match kernel::observe_root(&cx) {
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
    let runs = match history(&reader, &owner, limit.max(1)) {
        Observed::Verified(runs, _) => runs,
        Observed::Absent(_) => Vec::new(),
        Observed::Unknown(reason) => {
            view.state = "unknown".to_owned();
            view.warnings
                .push(format!("run journal is unreadable: {reason:?}"));
            return view;
        }
    };
    let gate = Gate::probe(&cx, &root, &owner);
    for (index, summary) in runs.iter().enumerate() {
        // Only the newest open run can hold the gate now; older open rows
        // are interrupted runs the next admission settles.
        let held = if index == 0 { Some(&gate) } else { None };
        let run = project(summary, held);
        if index == 0 {
            view.state = run.state.clone();
            if summary.phase != Phase::Closed {
                if run.state == "interrupted" {
                    view.next = Some(format!("effigy {}", target.label));
                }
                view.current = Some(run.clone());
            }
        }
        if view.latest.is_none() && summary.phase == Phase::Closed {
            view.latest = Some(run.clone());
        }
        view.history.push(run);
    }
    view
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
        run_id: summary.id.to_string(),
        state,
        phase: phase_name(summary.phase),
        profile,
        exit_code: (summary.phase == Phase::Closed)
            .then_some(exit_code)
            .flatten(),
        signal,
        cleanup: summary
            .cleanup_complete
            .map(|complete| if complete { "complete" } else { "incomplete" }),
        pid,
        started_at_epoch_ms: summary.created_at_ms,
        finished_at_epoch_ms: summary.closed_at_ms,
        duration_ms: summary
            .closed_at_ms
            .map(|closed| closed.saturating_sub(summary.created_at_ms)),
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
pub(in crate::runner) fn run_liveness(run_id: &str, next: &[&str]) -> RunLiveness {
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
    let root = match kernel::observe_root(&cx) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return RunLiveness::NotFound,
        Observed::Unknown(reason) => return RunLiveness::Unknown(format!("{reason:?}")),
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
