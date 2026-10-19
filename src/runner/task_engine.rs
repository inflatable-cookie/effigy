//! The task engine on the runtime kernel (contract 052 §10.1).
//!
//! Every task invocation resolves once to a `TaskTarget`, whose `TaskId` is
//! the run's owner. `Gate(Task)` excludes equivalent spellings and admits
//! distinct tasks independently; the run journal is the status record; a
//! plain host command runs under kernel custody. There is no lease, no
//! heartbeat, no unlock and no task cache.

mod checkout;
mod holders;
mod identity;
mod kernel;
mod legacy;
mod process;
mod run;
mod status;

pub(in crate::runner) use holders::{live_holders, HolderKind, LiveHolder};
pub(in crate::runner) use identity::TaskTarget;
pub use kernel::run_internal_kernel_entry;
pub(in crate::runner) use legacy::{legacy_families, refuse_live_legacy_holder};
pub(in crate::runner) use process::{run_plain, EnvOverride, PlainCommand, RunExec};
pub(in crate::runner) use run::{resolve_lock_wait_ms, TaskRun, TaskRunRequest};
pub(in crate::runner) use status::{
    run_liveness, task_status, RunLiveness, RunView, TaskStatusView, HISTORY_LIMIT,
};

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

#[cfg(test)]
mod tests;
