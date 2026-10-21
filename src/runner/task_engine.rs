//! The task engine on the runtime kernel (contract 052 §10.1).
//!
//! Every task invocation resolves once to a `TaskTarget`, whose `TaskId` is
//! the run's owner. `Gate(Task)` excludes equivalent spellings and admits
//! distinct tasks independently; the run journal is the status record; a
//! plain host command runs under kernel custody. There is no lease, no
//! heartbeat, no unlock and no task cache.

mod admission;
mod checkout;
mod control;
mod holders;
mod identity;
mod kernel;
mod legacy;
mod process;
mod remote;
mod run;
mod spawn_env;
mod status;

pub(in crate::runner) use control::{
    list as list_runs, lookup as lookup_run, read_log as read_run_log, request_stop,
    resolve as resolve_run, signal_owner, workload_process, Lookup, RunRecord, StopStarted,
    POLL as RUN_POLL,
};
pub(in crate::runner) use holders::{live_holders, HolderKind, LiveHolder};
pub(in crate::runner) use identity::TaskTarget;
pub use kernel::{adopt_signal_router, capture_startup_executable, run_internal_kernel_entry};
pub(in crate::runner) use legacy::{legacy_families, refuse_live_legacy_holder};
pub(in crate::runner) use process::{run_plain, EnvOverride, PlainCommand, PlainExit, RunExec};
pub(in crate::runner) use remote::{remote_tty, run_remote, RemoteTask};
pub(in crate::runner) use run::{resolve_lock_wait_ms, RunKey, TaskRun, TaskRunRequest};
pub(in crate::runner) use status::{
    run_liveness, task_status, RunLiveness, RunView, TaskStatusView, HISTORY_LIMIT,
};

#[cfg(test)]
pub(in crate::runner) use kernel::test_state_home;
pub(in crate::runner) use kernel::{interrupted_by, CustodyWait};
pub(in crate::runner) use spawn_env::{nested_capture, spawn_plain_env, SpawnScope};

/// The recorded outcome of a task command that exited with `code`.
pub(in crate::runner) fn workload_for_exit(code: i32) -> effigy_kernel::outcome::WorkloadOutcome {
    process::workload(Some(code), None)
}

/// The status of root task `task` under `root`, for tests.
#[cfg(test)]
pub(in crate::runner) fn root_task_status_for_tests(
    root: &std::path::Path,
    task: &str,
) -> TaskStatusView {
    let target = TaskTarget {
        root: root.to_path_buf(),
        catalog: String::new(),
        task: task.to_owned(),
        surface: effigy_core::task_selection::TaskSurface::Published,
        profile: None,
        shared_lock: None,
        label: task.to_owned(),
    };
    task_status(&target, HISTORY_LIMIT)
}

/// The status state of root task `task` under `root`, for tests that wait
/// on a live owner.
#[cfg(test)]
pub(in crate::runner) fn root_task_state_for_tests(root: &std::path::Path, task: &str) -> String {
    root_task_status_for_tests(root, task).state
}

/// Cancellation is checked before admission/spawn as well as while waiting.
/// A request between two steps must not start the next workload.
pub(in crate::runner) fn pending_stop(
    label: &str,
) -> Option<(i32, effigy_kernel::outcome::WorkloadOutcome)> {
    use effigy_kernel::{ctx::StopCause, outcome::WorkloadOutcome};
    let signals = kernel::poll_router();
    if let Some(signal) = signals
        .first()
        .map(|signal| signal.received)
        .or_else(kernel::interrupted_by)
    {
        return Some((128 + signal, workload_for_exit(128 + signal)));
    }
    if run::stop_requested() {
        return Some((
            1,
            WorkloadOutcome::Cancelled {
                cause: StopCause::Requested,
                signal: None,
            },
        ));
    }
    if spawn_env::remaining().is_some_and(|remaining| remaining.is_zero()) {
        return Some((
            124,
            WorkloadOutcome::TimedOut {
                phase: label.to_owned(),
            },
        ));
    }
    None
}

#[cfg(test)]
mod tests;
