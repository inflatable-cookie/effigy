//! Settle semantics for one managed headless run's stop.
//!
//! A stop, a signal settle and a handover-driven stop all report settled
//! only after the supervisor's whole stop cascade finished - including the
//! lifecycle cleanup's `container down` - and every task gate the run and
//! its children held is free. Holders come from the run journal plus a
//! fresh gate probe on this host (R1): a free gate is no holder, and an
//! unreadable probe is a holder (R3). The wait uses the derived managed
//! settle budget, never a fixed guess, so a real `container down` is not
//! mistaken for a hang. A refusal names what is still running and never
//! advises a stop that the caller itself already requested.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_kernel::id::Digest;
use effigy_managed::runtime::{
    managed_headless_stop, managed_owner_is_running, session_runtime_dir,
    ManagedHeadlessStopOutcome, MANAGED_SETTLE_BUDGET,
};

use crate::runner::error::RunnerError;
use crate::runner::task_engine::{live_holders, HolderKind, LiveHolder};

/// Extra settle budget per exec group marker the missing-record stop can
/// still see in the session's runtime directory, matching the record path's
/// per-child budget rule.
const MISSING_RECORD_EXEC_GROUP_BUDGET: Duration = Duration::from_secs(15);

/// Ask one managed headless run to stop and wait for its real settle: the
/// supervisor has exited or released its locks (an exited-but-unreaped
/// supervisor has already dropped both), nothing the run owns is left, and
/// every lock record attributed to the run's token is released through the
/// lock owner-token rules. Returns the settled summary; a genuine failure
/// to settle refuses and names what is still running.
///
/// A live run whose session record is missing or unreadable settles through
/// the same cascade: the stop request is delivered through the run's own
/// task/profile locks and run token, which the owner watches independently
/// of the record.
pub(in crate::runner) fn stop_and_settle_headless_run(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<String, RunnerError> {
    let (outcome, run_digest) = if effigy_managed::runtime::managed_headless_state_readable(
        repo_root, task_name, profile,
    ) {
        let outcome =
            managed_headless_stop(repo_root, task_name, profile).map_err(RunnerError::from)?;
        let digest = outcome
            .run_token
            .as_deref()
            .map(|token| Digest::of(token.as_bytes()));
        (outcome, digest)
    } else {
        stop_request_through_gates(repo_root, task_name, profile)?
    };
    // A stop reports settled only after the supervisor's whole stop cascade
    // finished: the host children are gone (in the request above), and the
    // supervisor only exits after its lifecycle cleanup brought the owned
    // environment down. The wait uses the derived settle budget, so a real
    // `container down` is never mistaken for a hang, and an
    // exited-but-unreaped supervisor (a zombie its parent has not collected
    // yet) settles as soon as its gates are free instead of stalling the
    // budget.
    // Runs the managed processes started (a remote Effigy task behind the
    // exec surface) were signalled by the cascade; each releases its gate when
    // its own owned-process grace ends, so they are awaited in the same budget.
    let deadline = std::time::Instant::now() + outcome.settle_budget;
    loop {
        let holders = live_holders(repo_root);
        let supervisor_still_holds = holders
            .iter()
            .any(|holder| holder.pid == Some(outcome.supervisor_pid));
        let run_gates_released = run_digest.is_none()
            || !holders
                .iter()
                .any(|holder| holder.managed_run == run_digest);
        let children_ended = outcome
            .surviving_processes
            .iter()
            .all(|(_, pid)| !effigy_process::process_is_running(*pid));
        if children_ended
            && run_gates_released
            && (!effigy_process::process_is_running(outcome.supervisor_pid)
                || !supervisor_still_holds)
        {
            break;
        }
        if std::time::Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let holders = live_holders(repo_root);
    let mut residue = Vec::new();
    let supervisor_still_holds = holders
        .iter()
        .any(|holder| holder.pid == Some(outcome.supervisor_pid));
    if supervisor_still_holds {
        residue.push(format!(
            "supervisor process {} is still running and holds its task gate",
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

    // A gate-found run knows only its token's digest; the owner's rewritten
    // record names the token itself.
    let run_token = outcome.run_token.clone().or_else(|| {
        effigy_managed::runtime::managed_session_run_token(repo_root, task_name, profile)
            .filter(|token| run_digest == Some(Digest::of(token.as_bytes())))
    });
    if let Some(error) = effigy_managed::runtime::managed_run_settle_error(
        repo_root,
        task_name,
        profile,
        run_token.as_deref(),
    )
    .map_err(RunnerError::from)?
    {
        return Err(RunnerError::task_invocation(format!(
            "managed task `{task_name}` stop did not settle: {error}"
        )));
    }

    // The cascade has ended: the run no longer holds anything itself. A
    // dead holder's gate is already free; refuse if a live process of the
    // run still holds one.
    let leftover = live_holders(repo_root)
        .into_iter()
        .filter(|holder| run_digest.is_some() && holder.managed_run == run_digest)
        .map(|holder| {
            format!(
                "task gate `{}` is still held ({})",
                holder.scope_label(),
                holder_pid_text(&holder)
            )
        })
        .collect::<Vec<_>>();
    if !leftover.is_empty() {
        return Err(did_not_settle(task_name, profile, &leftover));
    }

    Ok(format!(
        "settled managed headless task `{task_name}` (profile `{profile}`): {}",
        outcome.summary
    ))
}

fn did_not_settle(task_name: &str, profile: &str, residue: &[String]) -> RunnerError {
    RunnerError::task_invocation(format!(
        "managed task `{task_name}` (profile `{profile}`) did not settle after its stop was requested; still running: {}. The stop already in progress settles or fails on its own; retry once it has exited",
        residue.join("; ")
    ))
}

/// The live headless owner of this task/profile, judged from its run's
/// own gate: a run recorded with this profile first, then the task's run.
fn live_owner_from_gates(repo_root: &Path, task_name: &str, profile: &str) -> Option<LiveHolder> {
    let holders = live_holders(repo_root)
        .into_iter()
        .filter(|holder| holder.kind == HolderKind::Task && holder.task == task_name)
        .filter(|holder| holder.pid.is_some_and(managed_owner_is_running))
        .collect::<Vec<_>>();
    holders
        .iter()
        .find(|holder| holder.profile.as_deref() == Some(profile))
        .or_else(|| holders.iter().find(|holder| holder.profile.is_none()))
        .cloned()
}

fn holder_pid_text(holder: &LiveHolder) -> String {
    holder
        .pid
        .map(|pid| format!("pid {pid}"))
        .unwrap_or_else(|| "pid unreadable".to_owned())
}

fn no_live_run_error(task_name: &str, profile: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "no live managed run of task `{task_name}` (profile `{profile}`): its session record is missing and no live run holds its task gate; start it with `effigy {task_name} --headless`"
    ))
}

/// Deliver the stop request for a live run whose session record is missing
/// or unreadable: the owner watches `stop.requested` in its derived runtime
/// directory independently of the record, and its own cascade settles the
/// run and rewrites the record. The request refuses only when no live run
/// holds the task's gate.
fn stop_request_through_gates(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<(ManagedHeadlessStopOutcome, Option<Digest>), RunnerError> {
    let owner = live_owner_from_gates(repo_root, task_name, profile)
        .ok_or_else(|| no_live_run_error(task_name, profile))?;
    let owner_pid = owner
        .pid
        .ok_or_else(|| no_live_run_error(task_name, profile))?;
    let runtime_dir = session_runtime_dir(repo_root, task_name, profile);
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|error| RunnerError::task_invocation_failed_write(&runtime_dir, error))?;
    let stop_path = runtime_dir.join("stop.requested");
    std::fs::write(&stop_path, b"stop\n")
        .map_err(|error| RunnerError::task_invocation_failed_write(&stop_path, error))?;
    let exec_group_markers = exec_group_marker_count(&runtime_dir);
    Ok((
        ManagedHeadlessStopOutcome {
            summary: format!(
                "ended through its task gate (the session record was missing); {} recorded exec group(s) to end",
                exec_group_markers
            ),
            supervisor_pid: owner_pid,
            run_token: None,
            settle_budget: MANAGED_SETTLE_BUDGET
                + MISSING_RECORD_EXEC_GROUP_BUDGET * exec_group_markers as u32,
            surviving_processes: Vec::new(),
        },
        owner.managed_run,
    ))
}

fn exec_group_marker_count(runtime_dir: &Path) -> usize {
    fs::read_dir(runtime_dir.join("exec"))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "pgid"))
                .count()
        })
        .unwrap_or(0)
}

/// Gates establish liveness; a matching session only narrows the
/// environment. Missing records cannot grant destructive authority, so
/// unbound holders protect every environment in this checkout until they
/// settle.
pub(in crate::runner) fn environment_lock_holders(
    repo_root: &Path,
    environment: &str,
) -> Vec<LiveHolder> {
    let sessions = effigy_managed::runtime::live_headless_sessions(repo_root);
    live_holders(repo_root)
        .into_iter()
        .filter(|holder| {
            sessions
                .iter()
                .find(|session| {
                    holder.pid == Some(session.supervisor_pid)
                        && session
                            .run_token
                            .as_deref()
                            .is_some_and(|token| holder.carries_managed_run(token))
                })
                .and_then(|session| session.container_environment.as_deref())
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

/// Refuse a judged-failed generation's recovery teardown while a live
/// process of that run still holds a task gate: the owner may have been
/// misjudged dead through a stale or missing record, and tearing the
/// environment down would SIGKILL everything it is exec'ing into.
pub(in crate::runner) fn refuse_recovery_while_token_live(
    repo_root: &Path,
    token: &str,
) -> Result<(), RunnerError> {
    // An unreadable journal is an unknown holder: it never authorises the
    // teardown (R3).
    let holders = live_holders(repo_root)
        .into_iter()
        .filter(|holder| {
            matches!(holder.kind, HolderKind::Unreadable(_)) || holder.carries_managed_run(token)
        })
        .collect::<Vec<_>>();
    if holders.is_empty() {
        return Ok(());
    }
    let named = holders
        .iter()
        .map(|holder| format!("`{}` ({})", holder.scope_label(), holder_pid_text(holder)))
        .collect::<Vec<_>>()
        .join(", ");
    Err(RunnerError::task_invocation(format!(
        "managed run token `{token}` is judged failed, but its task gates are still held by live process(es) {named}; the recovery teardown was refused because the run may still be live. Stop it with `effigy <task> stop` first"
    )))
}

/// One process row of a missing-record status/logs view, named by its log
/// file (`NN-<name>.log`) because the record that carried its PID is gone.
struct LockViewProcess {
    name: String,
    log: PathBuf,
}

fn lock_view_processes(runtime_dir: &Path) -> Vec<LockViewProcess> {
    let mut rows = fs::read_dir(runtime_dir)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let path = entry.path();
                    if !path.is_file() {
                        return None;
                    }
                    let file_name = path.file_name()?.to_str()?;
                    let stem = file_name.strip_suffix(".log")?;
                    let (index, process) = stem.split_once('-')?;
                    // Log files are named `NN-<safe process name>.log`.
                    if index.len() != 2
                        || index.chars().any(|ch| !ch.is_ascii_digit())
                        || process.is_empty()
                    {
                        return None;
                    }
                    Some(LockViewProcess {
                        name: process.to_owned(),
                        log: path,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    rows.sort_by(|left, right| left.log.cmp(&right.log));
    rows
}

/// Report a live run from its locks and logs when its session record is
/// missing or unreadable. PIDs are not attributable without the record, so
/// the rows say so instead of inventing observations.
pub(in crate::runner) fn headless_status_from_locks(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<String, RunnerError> {
    let owner = live_owner_from_gates(repo_root, task_name, profile);
    let runtime_dir = session_runtime_dir(repo_root, task_name, profile);
    let rows = lock_view_processes(&runtime_dir);
    let Some(owner) = owner else {
        if rows.is_empty() {
            return Err(no_live_run_error(task_name, profile));
        }
        // Logs exist but no owner: report the residue honestly.
        let mut out = format!(
            "Managed Headless Status\ntask: {task_name}\nprofile: {profile}\nsession: stopped (session record missing; reported from logs)\nruntime-dir: {}\n\nprocess\tpid\tstatus\tlog\n",
            runtime_dir.display()
        );
        for row in &rows {
            out.push_str(&format!(
                "{}\t-\tunobserved\t{}\n",
                row.name,
                row.log.display()
            ));
        }
        return Ok(out);
    };
    let mut out = format!(
        "Managed Headless Status\ntask: {task_name}\nprofile: {profile}\nsession: running (session record missing; reported from its task gate)\nsupervisor-pid: {}\nruntime-dir: {}\n\nprocess\tpid\tstatus\tlog\n",
        holder_pid_text(&owner),
        runtime_dir.display()
    );
    for row in &rows {
        out.push_str(&format!(
            "{}\t-\tunobserved (session record missing)\t{}\n",
            row.name,
            row.log.display()
        ));
    }
    Ok(out)
}

/// Read or follow one live run's logs from the session directory when its
/// session record is missing or unreadable. The selection matches process
/// names by log file; following ends once the owner is gone and the files
/// stop growing.
pub(in crate::runner) fn headless_logs_from_locks(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
    process: Option<&str>,
    follow: bool,
) -> Result<String, RunnerError> {
    let runtime_dir = session_runtime_dir(repo_root, task_name, profile);
    let mut rows = lock_view_processes(&runtime_dir);
    if let Some(process) = process {
        rows.retain(|row| row.name == process);
        if rows.is_empty() {
            let available = lock_view_processes(&runtime_dir)
                .into_iter()
                .map(|row| row.name)
                .collect::<Vec<_>>()
                .join(", ");
            return Err(RunnerError::task_invocation(format!(
                "managed headless process `{process}` was not found (available: {available})"
            )));
        }
    }
    if rows.is_empty() {
        if live_owner_from_gates(repo_root, task_name, profile).is_none() {
            return Err(no_live_run_error(task_name, profile));
        }
        return Ok(String::new());
    }
    if !follow {
        let mut out = String::new();
        for row in &rows {
            let raw = fs::read_to_string(&row.log).unwrap_or_default();
            out.push_str(&format!("==> {} ({}) <==\n", row.name, row.log.display()));
            out.push_str(&raw);
            if !raw.ends_with('\n') {
                out.push('\n');
            }
        }
        return Ok(out);
    }
    let mut files = rows
        .iter()
        .map(|row| {
            fs::File::open(&row.log)
                .map(|file| (row.name.as_str(), file))
                .map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to follow managed log for `{}`: {error}",
                        row.name
                    ))
                })
        })
        .collect::<Result<Vec<_>, RunnerError>>()?;
    loop {
        let mut wrote = false;
        for (name, file) in &mut files {
            use std::io::Read;
            let mut chunk = String::new();
            file.read_to_string(&mut chunk).map_err(|error| {
                RunnerError::task_invocation(format!(
                    "failed to follow managed log for `{name}`: {error}"
                ))
            })?;
            if !chunk.is_empty() {
                print!("[{name}] {chunk}");
                use std::io::Write;
                std::io::stdout().flush().map_err(|error| {
                    RunnerError::task_invocation(format!(
                        "failed to flush managed logs output: {error}"
                    ))
                })?;
                wrote = true;
            }
        }
        if !wrote && live_owner_from_gates(repo_root, task_name, profile).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(String::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::task_engine::TaskRun;
    use effigy_kernel::outcome::WorkloadOutcome;

    fn hold_task(repo_root: &Path, token: Option<&str>) -> TaskRun {
        TaskRun::hold_for_tests(repo_root, "dev", token)
    }

    /// A judged-failed generation whose task gates are still held by a live
    /// process must not be torn down: the dead-owner judgment can be wrong
    /// through a stale or missing record.
    #[test]
    fn recovery_teardown_refuses_while_token_gates_are_live() {
        let private = tempfile::tempdir().expect("recovery refusal fixture");
        let held = hold_task(private.path(), Some("managedrun-live"));
        let error = refuse_recovery_while_token_live(private.path(), "managedrun-live")
            .expect_err("live holder must refuse the recovery teardown");
        let rendered = error.to_string();
        assert!(rendered.contains("managedrun-live"), "{rendered}");
        assert!(rendered.contains("task:dev"), "{rendered}");
        assert!(rendered.contains("refused"), "{rendered}");

        // A different token's live holder does not block this recovery.
        refuse_recovery_while_token_live(private.path(), "managedrun-other")
            .expect("another token's holder is not this generation");
        held.close(WorkloadOutcome::Succeeded);
    }

    #[test]
    fn recovery_teardown_proceeds_once_the_holder_has_exited() {
        let private = tempfile::tempdir().expect("dead holder fixture");
        hold_task(private.path(), Some("managedrun-dead")).abandon_for_tests();
        refuse_recovery_while_token_live(private.path(), "managedrun-dead")
            .expect("a free gate is no holder, whatever its row says");
    }

    #[test]
    fn environment_recovery_refuses_a_different_live_successor_token() {
        let private = tempfile::tempdir().expect("successor fixture");
        let held = hold_task(private.path(), Some("managedrun-successor"));
        // The old token alone appears recoverable, but the task's gate
        // belongs to a live successor. No down is allowed.
        refuse_recovery_while_token_live(private.path(), "managedrun-old")
            .expect("old holder ended");
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
