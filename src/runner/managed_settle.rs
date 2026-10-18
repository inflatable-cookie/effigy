//! Settle semantics for one managed headless run's stop.
//!
//! A stop, a signal settle and a handover-driven stop all report settled
//! only after the supervisor's whole stop cascade finished - including the
//! lifecycle cleanup's `container down` - and every task lock the run and
//! its children took is gone. The wait uses the derived managed settle
//! budget, never a fixed guess, so a real `container down` is not mistaken
//! for a hang. A refusal names what is still running and never advises a
//! stop that the caller itself already requested.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use effigy_builtin::LockScope;
use effigy_managed::runtime::{
    managed_headless_stop, managed_owner_is_running, session_runtime_dir,
    ManagedHeadlessStopOutcome, MANAGED_SETTLE_BUDGET,
};

use crate::runner::error::RunnerError;
use crate::runner::locking::io::{
    release_locks_attributed_to_run, task_lock_holders, TaskLockHolder,
};

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
    let outcome = if effigy_managed::runtime::managed_headless_state_readable(
        repo_root, task_name, profile,
    ) {
        managed_headless_stop(repo_root, task_name, profile).map_err(RunnerError::from)?
    } else {
        stop_request_through_locks(repo_root, task_name, profile)?
    };
    // A stop reports settled only after the supervisor's whole stop cascade
    // finished: the host children are gone (in the request above), and the
    // supervisor only exits after its lifecycle cleanup brought the owned
    // environment down. The wait uses the derived settle budget, so a real
    // `container down` is never mistaken for a hang, and an
    // exited-but-unreaped supervisor (a zombie its parent has not collected
    // yet) settles as soon as its lock records are gone instead of stalling
    // the budget.
    let deadline = std::time::Instant::now() + outcome.settle_budget;
    loop {
        let supervisor_still_holds = task_lock_holders(repo_root).into_iter().any(|holder| {
            holder.pid == outcome.supervisor_pid
                && pid_record_is_comparable_here(holder.hostname.as_deref())
        });
        let children_ended = outcome
            .surviving_processes
            .iter()
            .all(|(_, pid)| !effigy_process::process_is_running(*pid));
        if children_ended
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

    if let Some(error) = effigy_managed::runtime::managed_run_settle_error(
        repo_root,
        task_name,
        profile,
        outcome.run_token.as_deref(),
    )
    .map_err(RunnerError::from)?
    {
        return Err(RunnerError::task_invocation(format!(
            "managed task `{task_name}` stop did not settle: {error}"
        )));
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

/// The live headless owner of this task/profile, judged from its own lock
/// records alone: the profile lock first (only a managed profile run holds
/// it), then the task lock. Holders written by another host never name a
/// local process.
fn live_owner_from_locks(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Option<TaskLockHolder> {
    let mut scopes = vec![
        LockScope::Profile {
            task: task_name.to_owned(),
            profile: profile.to_owned(),
        },
        LockScope::Task(task_name.to_owned()),
    ];
    scopes.dedup();
    let holders = task_lock_holders(repo_root);
    scopes
        .iter()
        .filter_map(|scope| {
            let label = scope.label();
            holders
                .iter()
                .find(|holder| {
                    holder.scope == label
                        && pid_record_is_comparable_here(holder.hostname.as_deref())
                })
                .cloned()
        })
        .find(|holder| managed_owner_is_running(holder.pid))
}

fn no_live_run_error(task_name: &str, profile: &str) -> RunnerError {
    RunnerError::task_invocation(format!(
        "no live managed run of task `{task_name}` (profile `{profile}`): its session record is missing and no live task/profile lock names an owner; start it with `effigy {task_name} --headless`"
    ))
}

/// Deliver the stop request for a live run whose session record is missing
/// or unreadable: the owner watches `stop.requested` in its derived runtime
/// directory independently of the record, and its own cascade settles the
/// run, releases the run-token locks and rewrites the record. The request
/// refuses only when no live owner holds the task/profile locks.
fn stop_request_through_locks(
    repo_root: &Path,
    task_name: &str,
    profile: &str,
) -> Result<ManagedHeadlessStopOutcome, RunnerError> {
    let owner = live_owner_from_locks(repo_root, task_name, profile)
        .ok_or_else(|| no_live_run_error(task_name, profile))?;
    let runtime_dir = session_runtime_dir(repo_root, task_name, profile);
    std::fs::create_dir_all(&runtime_dir)
        .map_err(|error| RunnerError::task_invocation_failed_write(&runtime_dir, error))?;
    let stop_path = runtime_dir.join("stop.requested");
    std::fs::write(&stop_path, b"stop\n")
        .map_err(|error| RunnerError::task_invocation_failed_write(&stop_path, error))?;
    let exec_group_markers = exec_group_marker_count(&runtime_dir);
    Ok(ManagedHeadlessStopOutcome {
        summary: format!(
            "ended through its task/profile locks and run token (the session record was missing); {} recorded exec group(s) to end",
            exec_group_markers
        ),
        supervisor_pid: owner.pid,
        run_token: owner.managed_run_token.clone(),
        settle_budget: MANAGED_SETTLE_BUDGET
            + MISSING_RECORD_EXEC_GROUP_BUDGET * exec_group_markers as u32,
        surviving_processes: Vec::new(),
    })
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

/// Live managed-run lock holders of this checkout: task/profile lock records
/// carrying a managed run token whose holder is live on this host. This is
/// the discovery that does not need the session record, so a live owner whose
/// record is missing is still visible — the same discovery the missing-record
/// stop settles through.
pub(in crate::runner) fn live_managed_lock_holders(repo_root: &Path) -> Vec<TaskLockHolder> {
    task_lock_holders(repo_root)
        .into_iter()
        .filter(|holder| holder.scope.starts_with("task:") || holder.scope.starts_with("profile:"))
        .filter(|holder| holder.managed_run_token.is_some())
        .filter(|holder| pid_record_is_comparable_here(holder.hostname.as_deref()))
        .filter(|holder| effigy_managed::runtime::managed_owner_is_running(holder.pid))
        .collect()
}

/// Refuse a judged-failed generation's recovery teardown while a live
/// process still holds one of its run-token locks: the owner may have been
/// misjudged dead through a stale or missing record, and tearing the
/// environment down would SIGKILL everything it is exec'ing into.
pub(in crate::runner) fn refuse_recovery_while_token_live(
    repo_root: &Path,
    token: &str,
) -> Result<(), RunnerError> {
    let holders = live_managed_lock_holders(repo_root)
        .into_iter()
        .filter(|holder| holder.managed_run_token.as_deref() == Some(token))
        .collect::<Vec<_>>();
    if holders.is_empty() {
        return Ok(());
    }
    let named = holders
        .iter()
        .map(|holder| format!("`{}` (pid {})", holder.scope, holder.pid))
        .collect::<Vec<_>>()
        .join(", ");
    Err(RunnerError::task_invocation(format!(
        "managed run token `{token}` is judged failed, but its task locks are still held by live process(es) {named}; the recovery teardown was refused because the run may still be live. Stop it with `effigy <task> stop` first"
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
    let owner = live_owner_from_locks(repo_root, task_name, profile);
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
        "Managed Headless Status\ntask: {task_name}\nprofile: {profile}\nsession: running (session record missing; reported from task/profile locks)\nsupervisor-pid: {}\nruntime-dir: {}\n\nprocess\tpid\tstatus\tlog\n",
        owner.pid,
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
        if live_owner_from_locks(repo_root, task_name, profile).is_none() {
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
        if !wrote && live_owner_from_locks(repo_root, task_name, profile).is_none() {
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(String::new())
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

    fn write_holder_lock(repo_root: &Path, pid: u32, token: &str, hostname: Option<&str>) {
        let locks = repo_root.join(".effigy/locks");
        std::fs::create_dir_all(&locks).expect("locks dir");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        let hostname = hostname
            .map(|host| format!(r#""hostname":"{host}","#))
            .unwrap_or_default();
        std::fs::write(
            locks.join("task-dev.lock"),
            format!(
                r#"{{"scope":"task:dev","pid":{pid},"started_at_epoch_ms":{now},{hostname}"managed_run_token":"{token}"}}"#
            ),
        )
        .expect("holder record");
    }

    /// A judged-failed generation whose run-token locks are still held by a
    /// live process must not be torn down: the dead-owner judgment can be
    /// wrong through a stale or missing record.
    #[test]
    fn recovery_teardown_refuses_while_token_locks_are_live() {
        let private = tempfile::tempdir().expect("recovery refusal fixture");
        write_holder_lock(private.path(), std::process::id(), "managedrun-live", None);
        let error = refuse_recovery_while_token_live(private.path(), "managedrun-live")
            .expect_err("live holder must refuse the recovery teardown");
        let rendered = error.to_string();
        assert!(rendered.contains("managedrun-live"), "{rendered}");
        assert!(rendered.contains("task:dev"), "{rendered}");
        assert!(rendered.contains("refused"), "{rendered}");

        // A different token's live holder does not block this recovery.
        refuse_recovery_while_token_live(private.path(), "managedrun-other")
            .expect("another token's holder is not this generation");
    }

    #[test]
    fn recovery_teardown_proceeds_for_actually_dead_holders() {
        let private = tempfile::tempdir().expect("dead holder fixture");
        write_holder_lock(private.path(), 999_999_999, "managedrun-dead", None);
        refuse_recovery_while_token_live(private.path(), "managedrun-dead")
            .expect("a dead holder grants teardown authority");
    }

    /// The live-owner discovery sees a live holder through its run token even
    /// when its session record is missing.
    #[test]
    fn live_managed_lock_holders_sees_live_token_holders() {
        let private = tempfile::tempdir().expect("discovery fixture");
        write_holder_lock(
            private.path(),
            std::process::id(),
            "managedrun-live",
            Some("not-this-host-namespace"),
        );
        assert!(
            live_managed_lock_holders(private.path()).is_empty(),
            "a foreign-host holder names no local process"
        );
        write_holder_lock(private.path(), std::process::id(), "managedrun-live", None);
        let holders = live_managed_lock_holders(private.path());
        assert_eq!(holders.len(), 1);
        assert_eq!(holders[0].scope, "task:dev");
        assert_eq!(
            holders[0].managed_run_token.as_deref(),
            Some("managedrun-live")
        );
    }
}
