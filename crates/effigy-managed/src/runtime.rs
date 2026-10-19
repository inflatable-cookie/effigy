use std::path::Path;

use effigy_process::ProcessSupervisor;
use effigy_tui::multiprocess::{
    run_multiprocess_tui_with_control, MultiProcessTuiControl, MultiProcessTuiOptions,
};

use super::render_support::managed_process_specs;
use crate::ManagedError;
use crate::ManagedTaskPlan;

#[path = "runtime/headless.rs"]
mod headless;
#[path = "runtime/policy.rs"]
mod policy;
#[path = "runtime/redaction.rs"]
mod redaction;

pub use headless::{
    clear_managed_run_failure, exec_group_file_path, failed_managed_run_token,
    failed_managed_run_tokens, live_headless_sessions, managed_headless_logs,
    managed_headless_state_readable, managed_headless_status, managed_headless_stop,
    managed_owner_is_running, managed_run_settle_error, managed_session_run_token,
    record_managed_run_settle, run_managed_task_headless, session_runtime_dir, LiveHeadlessSession,
    ManagedHeadlessStopOutcome, LIFECYCLE_CLEANUP_TIMEOUT_SECS, MANAGED_SETTLE_BUDGET,
};

pub fn run_managed_task_tui(
    task_name: &str,
    repo_root: &Path,
    plan: ManagedTaskPlan,
    settle: Option<&crate::ManagedRunSettle<'_>>,
) -> Result<String, ManagedError> {
    let session = headless::TuiSessionControl::new(task_name, repo_root, &plan)?;
    let owns_environment = plan
        .processes
        .iter()
        .any(|process| process.role == crate::ManagedProcessRole::Lifecycle);
    let ManagedTaskPlan {
        processes,
        tab_order,
        fail_on_non_zero,
        profile,
        ..
    } = plan;
    let specs = managed_process_specs(processes);
    let poll = |supervisor: &ProcessSupervisor| session.poll(supervisor);
    let before_shutdown = |supervisor: &ProcessSupervisor| {
        session.poll(supervisor)?;
        let result = session.end_exec_groups(settle);
        // The runner's final down is the backstop for an owned environment.
        if owns_environment {
            Ok(())
        } else {
            result
        }
    };
    let after_shutdown = |supervisor: &ProcessSupervisor| session.finish(supervisor);
    let control = MultiProcessTuiControl {
        poll: &poll,
        before_shutdown: &before_shutdown,
        after_shutdown: &after_shutdown,
    };
    let outcome = run_multiprocess_tui_with_control(
        repo_root.to_path_buf(),
        specs,
        tab_order,
        MultiProcessTuiOptions::default(),
        Some(&control),
    )
    .map_err(|error| {
        ManagedError::Ui(format!(
            "managed tui runtime failed for task `{task_name}`: {error}"
        ))
    })?;
    policy::enforce_non_zero_exit_policy(
        task_name,
        &profile,
        fail_on_non_zero,
        if outcome.stopped_by_user {
            Vec::new()
        } else {
            outcome.non_zero_exits
        },
    )?;
    Ok(String::new())
}
