//! Environment holder rules for managed runs (contract 052 R4, I-A2).
//!
//! A live task gate protects the environments its run is bound to. Gates
//! establish liveness; a managed run's own journal names the environment it
//! holds (its `managed:environment` effect). A holder whose binding cannot be
//! read protects every environment of the checkout (R3). Only the bound
//! run's own settle, exempt by its run token, may bring its environment down.
//!
//! SEAM(v015-environment): these rules are the narrow adapter over the
//! current borrow behaviour. The environment record's hold API replaces them
//! with `Attachments::holders` and `permit`.

use std::path::Path;

use crate::runner::error::RunnerError;
use crate::runner::task_engine::{live_holders, LiveHolder};

fn holder_pid_text(holder: &LiveHolder) -> String {
    holder
        .pid
        .map(|pid| format!("pid {pid}"))
        .unwrap_or_else(|| "pid unreadable".to_owned())
}

/// Live holders that protect `environment`: every holder whose run journal
/// does not bind it to another environment.
pub(in crate::runner) fn environment_lock_holders(
    repo_root: &Path,
    environment: &str,
) -> Vec<LiveHolder> {
    live_holders(repo_root)
        .into_iter()
        .filter(|holder| {
            holder
                .managed_run
                .as_ref()
                .and(holder.run)
                .and_then(crate::runner::managed_run::bound_environment)
                .is_none_or(|bound| bound == environment)
        })
        .collect()
}

/// Only the run's own stop may mutate its environment under its live gates.
/// Pre-spawn recovery may exempt its newly acquired local owner lock; a
/// handover may exempt the incoming run, whose environment is not the target.
pub(in crate::runner) fn refuse_environment_mutation(
    repo_root: &Path,
    environment: &str,
    exempt_pid: Option<u32>,
    exempt_token: Option<&str>,
) -> Result<(), RunnerError> {
    let holders = environment_lock_holders(repo_root, environment)
        .into_iter()
        .filter(|holder| {
            !exempt_pid.is_some_and(|caller| {
                holder.pid.is_some_and(|pid| {
                    caller == pid
                        || (holder.managed_run.is_none()
                            && effigy_process::process_is_descendant_of(caller, pid))
                })
            })
        })
        .filter(|holder| exempt_token.is_none_or(|token| !holder.carries_managed_run(token)))
        .map(|holder| format!("`{}` ({})", holder.scope_label(), holder_pid_text(&holder)))
        .collect::<Vec<_>>();
    if holders.is_empty() {
        return Ok(());
    }
    Err(RunnerError::task_invocation(format!(
        "refused to stop, restart or recreate environment `{environment}` while live task runs hold their gates: {}. End the bound run through its own stop first",
        holders.join(", ")
    )))
}

/// Mount reconciliation runs before the incoming task's container workload.
/// Its own ordinary task gate is admitted, but is not a bound workload yet.
/// Exempt only that exact process, never an ancestor, managed run or unknown
/// holder. Explicit CLI up has no such ordinary gate of its own.
pub(in crate::runner) fn refuse_pre_spawn_mount_reconcile(
    repo_root: &Path,
    environment: &str,
) -> Result<(), RunnerError> {
    if environment_lock_holders(repo_root, environment)
        .iter()
        .all(|holder| holder.pid == Some(std::process::id()) && holder.managed_run.is_none())
    {
        return Ok(());
    }
    refuse_environment_mutation(repo_root, environment, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::{HolderKind, TaskRun};
    use effigy_kernel::outcome::WorkloadOutcome;

    fn hold_task(repo_root: &Path, token: Option<&str>) -> TaskRun {
        TaskRun::hold_for_tests(repo_root, "dev", token)
    }

    #[test]
    fn environment_recovery_refuses_a_different_live_successor_token() {
        let private = tempfile::tempdir().expect("successor fixture");
        let held = hold_task(private.path(), Some("managedrun-successor"));
        let error = refuse_environment_mutation(private.path(), "stack", None, None)
            .expect_err("successor protects environment");
        assert!(error.to_string().contains("task:dev"));
        held.close(WorkloadOutcome::Succeeded);
    }

    #[test]
    fn isolated_dirs_pre_spawn_exemption_never_exempts_a_managed_holder() {
        let private = tempfile::tempdir().expect("mount preparation fixture");
        let held = hold_task(private.path(), None);
        refuse_pre_spawn_mount_reconcile(private.path(), "stack")
            .expect("own ordinary preparation gate");
        refuse_environment_mutation(private.path(), "stack", None, None)
            .expect_err("general teardown keeps its stricter guard");
        held.close(WorkloadOutcome::Succeeded);
        let managed = hold_task(private.path(), Some("managedrun-bound"));
        refuse_pre_spawn_mount_reconcile(private.path(), "stack")
            .expect_err("even a same-process managed holder remains protected");
        managed.close(WorkloadOutcome::Succeeded);
    }

    #[cfg(unix)]
    #[test]
    fn explicit_stop_exempts_its_calling_host_task_but_not_a_managed_ancestor() {
        let private = tempfile::tempdir().expect("calling task fixture");
        // This process holds the gate; a child of it stands in for the
        // explicit `down` running inside that task.
        let mut caller = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("caller fixture");
        let held = hold_task(private.path(), Some("managedrun-parent"));
        refuse_environment_mutation(private.path(), "stack", Some(caller.id()), None)
            .expect_err("a managed ancestor must use its own stop token");
        held.close(WorkloadOutcome::Succeeded);

        let held = hold_task(private.path(), None);
        refuse_environment_mutation(private.path(), "stack", Some(caller.id()), None)
            .expect("an explicit down may finish its own calling host task");
        refuse_environment_mutation(private.path(), "stack", None, None)
            .expect_err("a reaper cannot borrow the calling-task exemption");
        held.close(WorkloadOutcome::Succeeded);
        let _ = caller.kill();
        let _ = caller.wait();
    }

    /// The live-owner discovery sees a live holder through its run token even
    /// when its session record is missing.
    #[test]
    fn live_holders_see_live_token_holders() {
        let managed = |root: &Path| -> Vec<LiveHolder> {
            live_holders(root)
                .into_iter()
                .filter(|holder| holder.kind == HolderKind::Task && holder.managed_run.is_some())
                .collect()
        };
        let private = tempfile::tempdir().expect("discovery fixture");
        let plain = hold_task(private.path(), None);
        assert!(
            managed(private.path()).is_empty(),
            "a holder without a managed run is not a managed holder"
        );
        plain.close(WorkloadOutcome::Succeeded);
        let held = hold_task(private.path(), Some("managedrun-live"));
        let holders = managed(private.path());
        assert_eq!(holders.len(), 1);
        assert_eq!(holders[0].scope_label(), "task:dev");
        assert!(holders[0].carries_managed_run("managedrun-live"));
        assert_eq!(holders[0].pid, Some(std::process::id()));
        held.close(WorkloadOutcome::Succeeded);
    }
}
