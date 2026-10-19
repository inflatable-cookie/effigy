//! Generic run control over the kernel run journal (Q-006, contract 052
//! §5.6): every run, heavy or not, is found by its run id or `--run-id` key,
//! read from its row and owner gate, followed through its logs and stopped
//! through a fenced kernel request. Reads are pure (R2); only `stop` writes.

use std::path::Path;
use std::time::Duration;

use effigy_kernel::custody::Stream;
use effigy_kernel::gate::Gate;
use effigy_kernel::id::{ResourceKind, RunId, Scope};
use effigy_kernel::observe::Observed;
use effigy_kernel::run::{Fence, LogRead, Phase, Request, RequestAction, RunSummary};
use effigy_kernel::store::Store;

use super::kernel::{self, TaskSettlement};
use super::status::{project_run, RunView};
use super::RunKey;
use crate::runner::error::RunnerError;

/// Process labels that are part of a run but not its workload output.
const NOT_WORKLOAD: &[&str] = &[super::admission::HOOK_LABEL, super::process::ENV_EXEC_LABEL];
const KERNEL_PROCESS: &str = "kernel:process";

/// One run as the journal and its owner gate report it now.
#[derive(Debug, Clone)]
pub(in crate::runner) struct RunRecord {
    pub(in crate::runner) id: RunId,
    pub(in crate::runner) summary: RunSummary,
    pub(in crate::runner) view: RunView,
    /// The selector the run was admitted for.
    pub(in crate::runner) task: String,
}

impl RunRecord {
    /// Closed, or open with a free owner gate: either way it will make no
    /// more progress by itself.
    pub(in crate::runner) fn terminal(&self) -> bool {
        matches!(self.view.state.as_str(), "interrupted") || self.summary.phase == Phase::Closed
    }

    /// The run's own result as a process exit code.
    pub(in crate::runner) fn exit_code(&self) -> i32 {
        match self.view.exit_code {
            Some(code) if self.summary.phase == Phase::Closed => code,
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
pub(in crate::runner) fn resolve(selector: &str) -> Result<RunId, RunnerError> {
    if let Ok(id) = selector.parse::<RunId>() {
        if !matches!(lookup(id), Lookup::NotFound) {
            return Ok(id);
        }
    }
    match RunKey::parse(selector) {
        Ok(key) => Ok(key.id()),
        Err(_) => selector.parse::<RunId>().map_err(|_| {
            RunnerError::task_invocation(format!("`{selector}` is neither a run id nor a run key"))
        }),
    }
}

pub(in crate::runner) fn lookup(id: RunId) -> Lookup {
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx) {
        Observed::Verified(root, _) => root,
        Observed::Absent(_) => return Lookup::NotFound,
        Observed::Unknown(reason) => return Lookup::Unknown(format!("{reason:?}")),
    };
    let reader = match Store::read(&cx, &root) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Lookup::NotFound,
        Observed::Unknown(reason) => return Lookup::Unknown(format!("{reason:?}")),
    };
    match effigy_kernel::run::summary(&reader, id) {
        Observed::Verified(summary, _) => {
            Lookup::Found(Box::new(record(&cx, &root, summary)))
        }
        Observed::Absent(_) => Lookup::NotFound,
        Observed::Unknown(reason) => Lookup::Unknown(format!("{reason:?}")),
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
        summary,
        view,
        task,
    }
}

/// The newest runs of tasks and QA groups, in the checkout at `root`, or on
/// the whole host when `root` is `None`. Internal lock sections are hidden.
pub(in crate::runner) fn list(
    root: Option<&Path>,
    limit: usize,
) -> Result<Vec<RunRecord>, RunnerError> {
    let scope = match root {
        Some(root) => match super::checkout::read(root)
            .map_err(|error| unreadable(&format!("checkout identity: {error}")))?
        {
            Some(checkout) => Some(Scope::Checkout(checkout)),
            None => return Ok(Vec::new()),
        },
        None => None,
    };
    let cx = kernel::observe();
    let state = match kernel::observe_root(&cx) {
        Observed::Verified(state, _) => state,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => return Err(unreadable(&format!("{reason:?}"))),
    };
    let reader = match Store::read(&cx, &state) {
        Observed::Verified(reader, _) => reader,
        Observed::Absent(_) => return Ok(Vec::new()),
        Observed::Unknown(reason) => return Err(unreadable(&format!("{reason:?}"))),
    };
    // Lock sections share the scope; read past them to fill the page.
    let rows = match effigy_kernel::run::recent(&reader, scope, limit.saturating_mul(4).max(64)) {
        Observed::Verified(rows, _) => rows,
        Observed::Absent(_) => Vec::new(),
        Observed::Unknown(reason) => return Err(unreadable(&format!("{reason:?}"))),
    };
    Ok(rows
        .into_iter()
        .map(|summary| record(&cx, &state, summary))
        .filter(|record| {
            let owner_is_task = record.summary.invocation.as_ref().is_some_and(|invocation| {
                invocation.spec().owner.kind() == ResourceKind::Task
            });
            owner_is_task || record.task.starts_with("qa-group:")
        })
        .take(limit)
        .collect())
}

fn unreadable(reason: &str) -> RunnerError {
    RunnerError::task_invocation(format!("the run journal is unreadable: {reason}"))
}

/// The run's workload process: its last custody process that is not the
/// admission hook or an env-schema command. `None` until it has started.
pub(in crate::runner) fn workload_process(id: RunId) -> Observed<Option<i64>> {
    let cx = kernel::observe();
    let root = match kernel::observe_root(&cx) {
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
    match kernel::observe_root(&cx) {
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

/// The kernel stop path: a generation-fenced `Stop` request the live owner
/// acts on, or, when the owner is gone, the one settle under its gate.
pub(in crate::runner) fn request_stop(record: &RunRecord) -> Result<StopStarted, RunnerError> {
    if record.summary.phase == Phase::Closed {
        return Ok(StopStarted::AlreadyClosed);
    }
    let invocation = record.summary.invocation.as_ref().ok_or_else(|| {
        RunnerError::task_invocation(format!(
            "run {} has no readable invocation; it cannot be addressed",
            record.id
        ))
    })?;
    let owner = invocation.spec().owner.clone();
    let root = kernel::effects_root()?;
    let cx = kernel::effects();
    if record.view.state == "interrupted" {
        let cleanup = kernel::cleanup()?;
        let next = kernel::next_invocation(&record.task)?;
        return effigy_kernel::run::settle_interrupted(
            &cx,
            root,
            &owner,
            record.id,
            next,
            &cleanup,
            &mut TaskSettlement,
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
            generation: record.summary.generation,
        },
        action: RequestAction::Stop,
    };
    effigy_kernel::run::request(&cx, root, &request)
        .map(|()| StopStarted::Requested)
        .map_err(|error| {
            RunnerError::task_invocation(format!(
                "run {} did not accept a stop request: {error}",
                record.id
            ))
        })
}

/// For an owner that does not read stop requests (a sequence, managed or
/// container run): TERM to the gate holder named by the owner gate, whose
/// start identity the gate probe verified. Its signal router forwards the
/// stop to its own process groups. Never a pattern kill.
pub(in crate::runner) fn signal_owner(record: &RunRecord) -> bool {
    let Some(invocation) = record.summary.invocation.as_ref() else {
        return false;
    };
    let Ok(root) = kernel::effects_root() else {
        return false;
    };
    matches!(
        Gate::signal_holder(
            &kernel::effects(),
            root,
            &invocation.spec().owner,
            &record.id,
            libc::SIGTERM,
        ),
        Observed::Verified(..)
    )
}

/// Bound for one journal poll while following or stopping.
pub(in crate::runner) const POLL: Duration = Duration::from_millis(100);
