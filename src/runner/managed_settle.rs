//! Settle semantics for one managed headless run's stop.
//!
//! A stop, a signal settle and a handover-driven stop all report settled
//! only after the supervisor's whole stop cascade finished - including the
//! lifecycle cleanup's `container down` - and every task lock the run and
//! its children took is gone. The wait uses the derived managed settle
//! budget, never a fixed guess, so a real `container down` is not mistaken
//! for a hang. A refusal names what is still running and never advises a
//! stop that the caller itself already requested.

use std::path::Path;

use effigy_managed::runtime::managed_headless_stop;

use crate::runner::error::RunnerError;
use crate::runner::locking::io::{release_locks_attributed_to_run, task_lock_holders};

/// Ask one managed headless run to stop and wait for its real settle: the
/// supervisor has exited or released its locks (an exited-but-unreaped
/// supervisor has already dropped both), nothing the run owns is left, and
/// every lock record attributed to the run's token is released through the
/// lock owner-token rules. Returns the settled summary; a genuine failure
/// to settle refuses and names what is still running.
pub(in crate::runner) fn stop_and_settle_headless_run(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<String, RunnerError> {
    let outcome =
        managed_headless_stop(repo_root, task_name, profile).map_err(RunnerError::from)?;
    // A stop reports settled only after the supervisor's whole stop cascade
    // finished: the host children are gone (in the request above), and the
    // supervisor only exits after its lifecycle cleanup brought the owned
    // environment down. The wait uses the derived settle budget, so a real
    // `container down` is never mistaken for a hang, and an
    // exited-but-unreaped supervisor (a zombie its parent has not collected
    // yet) settles as soon as its lock records are gone instead of stalling
    // the budget.
    let deadline = std::time::Instant::now() + effigy_managed::runtime::MANAGED_SETTLE_BUDGET;
    loop {
        let supervisor_still_holds = task_lock_holders(repo_root).into_iter().any(|holder| {
            holder.pid == outcome.supervisor_pid
                && pid_record_is_comparable_here(holder.hostname.as_deref())
        });
        if !effigy_process::process_is_running(outcome.supervisor_pid) || !supervisor_still_holds {
            break;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let holders = task_lock_holders(repo_root);
    let mut residue = Vec::new();
    let supervisor_still_holds = holders.iter().any(|holder| {
        holder.pid == outcome.supervisor_pid
            && pid_record_is_comparable_here(holder.hostname.as_deref())
    });
    if supervisor_still_holds {
        residue.push(format!(
            "supervisor process {} is still running and holds its task lock",
            outcome.supervisor_pid
        ));
    }
    for (name, pid) in &outcome.surviving_processes {
        if effigy_process::process_is_running(*pid) {
            residue.push(format!("process `{name}` (pid {pid}) is still running"));
        }
    }
    if !residue.is_empty() {
        return Err(did_not_settle(task_name, profile, &residue));
    }

    // The cascade has ended: the run no longer holds anything itself.
    // Release the records its SIGKILLed holders could not drop, then
    // refuse if any attributed record still survives the sweep.
    let released = release_run_lock_residue(repo_root, outcome.run_token.as_deref());
    let leftover = task_lock_holders(repo_root)
        .into_iter()
        .filter(|holder| {
            outcome
                .run_token
                .as_deref()
                .is_some_and(|token| holder.managed_run_token.as_deref() == Some(token))
        })
        .map(|holder| {
            format!(
                "task lock `{}` is still held (pid {}{})",
                holder.scope,
                holder.pid,
                holder
                    .hostname
                    .as_deref()
                    .map(|host| format!(", host {host}"))
                    .unwrap_or_default()
            )
        })
        .collect::<Vec<_>>();
    if !leftover.is_empty() {
        return Err(did_not_settle(task_name, profile, &leftover));
    }

    let mut summary = format!(
        "settled managed headless task `{task_name}` (profile `{profile}`): {}",
        outcome.summary
    );
    if !released.is_empty() {
        summary.push_str(&format!("; released lock(s) {}", released.join(", ")));
    }
    Ok(summary)
}

fn did_not_settle(task_name: &str, profile: &str, residue: &[String]) -> RunnerError {
    RunnerError::task_invocation(format!(
        "managed task `{task_name}` (profile `{profile}`) did not settle after its stop was requested; still running: {}. The stop already in progress settles or fails on its own; retry once it has exited",
        residue.join("; ")
    ))
}

/// Release the lock records still attributed to the run's token.
pub(in crate::runner) fn release_run_lock_residue(
    repo_root: &Path,
    run_token: Option<&str>,
) -> Vec<String> {
    let Some(run_token) = run_token else {
        return Vec::new();
    };
    release_locks_attributed_to_run(repo_root, run_token, None)
}

/// Whether this host's process table can judge a record's holder. PIDs are
/// only comparable inside the PID namespace that wrote the record: a
/// container-side holder's PID means nothing on the host, and a host
/// holder's PID means nothing inside a container. A record annotated with
/// a different hostname is judged by its heartbeat lease elsewhere, never
/// by a PID match here.
fn pid_record_is_comparable_here(written: Option<&str>) -> bool {
    let Some(written) = written else {
        // Unannotated records keep today's behavior.
        return true;
    };
    let local = crate::runner::locking::io::lock_hostname();
    local.is_some_and(|local| local == written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unannotated_hostname_is_always_comparable() {
        assert!(pid_record_is_comparable_here(None));
    }

    #[test]
    fn foreign_hostname_is_never_comparable() {
        assert!(!pid_record_is_comparable_here(Some(
            "not-this-host-namespace"
        )));
    }
}
