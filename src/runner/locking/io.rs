use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::model::LockScope;
use crate::runner::error::RunnerError;

pub(in crate::runner) use effigy_builtin::UnlockResult;

#[path = "io/files.rs"]
mod files;
#[path = "io/paths.rs"]
mod paths;
#[path = "io/stale.rs"]
mod stale;

const LOCKS_DIR: &str = ".effigy/locks";
pub(in crate::runner) const LOCK_WAIT_ENV: &str = "EFFIGY_LOCK_WAIT_MS";
const LOCK_WAIT_POLL_MS: u64 = 50;

pub(in crate::runner) use stale::lock_hostname;

pub(in crate::runner) fn resolve_lock_wait_ms(explicit: Option<u64>) -> Result<u64, RunnerError> {
    if let Some(value) = explicit {
        return Ok(value);
    }
    match std::env::var(LOCK_WAIT_ENV) {
        Ok(raw) => {
            let raw = raw.trim();
            if raw.is_empty() {
                return Ok(0);
            }
            raw.parse::<u64>().map_err(|_| {
                RunnerError::task_invocation(format!(
                    "`{LOCK_WAIT_ENV}` must be a non-negative integer millisecond count"
                ))
            })
        }
        Err(_) => Ok(0),
    }
}

/// An unparsable lock younger than this may still be mid-write by a release
/// that publishes non-atomically; only an older one is treated as abandoned.
const UNPARSABLE_LOCK_GRACE: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub(in crate::runner) struct LockGuard {
    path: PathBuf,
    token: String,
    stop_heartbeat: Option<Sender<()>>,
    heartbeat_thread: Option<JoinHandle<()>>,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if let Some(stop) = self.stop_heartbeat.take() {
            let _ = stop.send(());
        }
        if let Some(handle) = self.heartbeat_thread.take() {
            let _ = handle.join();
        }
        // Never remove a lock that a successor now owns.
        if owns_lock(&self.path, &self.token) {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LockRecord {
    scope: String,
    pid: u32,
    started_at_epoch_ms: u128,
    #[serde(default)]
    heartbeat_at_epoch_ms: u128,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    workspace_root: Option<String>,
    /// Identifies the guard that published this record, so heartbeat, release
    /// and stale cleanup only touch the record they mean to.
    #[serde(default)]
    owner_token: Option<String>,
    /// Token of the managed run whose process tree wrote this record, when
    /// the holder is part of a managed run (host children and the exec'd
    /// in-container processes alike). The run's settle releases these
    /// records through the owner-token rules, and holders across a PID
    /// namespace boundary are judged by their heartbeat lease instead of a
    /// foreign process table. Records without the field behave exactly as
    /// before.
    #[serde(default)]
    managed_run_token: Option<String>,
}

fn new_owner_token() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        stale::now_epoch_ms(),
        COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn owns_lock(path: &Path, token: &str) -> bool {
    files::read_lock_record(path).is_ok_and(|record| record.owner_token.as_deref() == Some(token))
}

/// Publish `body` at `path` only if no lock exists, without ever exposing a
/// partially written record: the bytes go to a private sibling first and are
/// hard-linked into place, which fails if `path` already exists.
fn publish_new_lock(path: &Path, body: &[u8]) -> std::io::Result<bool> {
    let staged = files::staging_path(path);
    fs::write(&staged, body)?;
    let linked = fs::hard_link(&staged, path);
    let _ = fs::remove_file(&staged);
    match linked {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        // Filesystems without hard links keep the exclusive-create fallback.
        Err(_) => match OpenOptions::new().create_new(true).write(true).open(path) {
            Ok(mut file) => file.write_all(body).map(|()| true),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
            Err(error) => Err(error),
        },
    }
}

/// Remove a lock judged stale only if it still holds exactly that record.
fn remove_if_unchanged(path: &Path, judged: &LockRecord) {
    if files::read_lock_record(path).is_ok_and(|current| current == *judged) {
        let _ = fs::remove_file(path);
    }
}

/// Remove an unparsable lock only if it still holds exactly the judged bytes.
/// A successor's just-published complete record has different bytes, so the
/// comparison refuses to unlink it; two reclaimers racing the same corrupt
/// record still serialize on the unlink, and the loser retries against the
/// winner's fresh lock.
fn remove_unparsable_if_unchanged(path: &Path, judged: &[u8]) -> bool {
    // The judged bytes were already deemed corrupt; if they now parse, a
    // successor published between the judge and the re-read.
    if serde_json::from_slice::<LockRecord>(judged).is_ok() {
        return false;
    }
    match fs::read(path) {
        Ok(current) if current == judged => {
            if !unparsable_lock_is_abandoned(path) {
                return false;
            }
            fs::remove_file(path).is_ok()
        }
        _ => false,
    }
}

fn unparsable_lock_is_abandoned(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= UNPARSABLE_LOCK_GRACE)
}

pub(in crate::runner) fn acquire_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<Vec<LockGuard>, RunnerError> {
    acquire_scopes_with_wait(workspace_root, scopes, 0)
}

pub(in crate::runner) fn acquire_scopes_with_wait(
    workspace_root: &Path,
    scopes: &[LockScope],
    wait_timeout_ms: u64,
) -> Result<Vec<LockGuard>, RunnerError> {
    let mut unique_scopes = scopes.to_vec();
    unique_scopes.sort();
    unique_scopes.dedup();

    let locks_root = paths::ensure_locks_root(workspace_root)?;

    let mut guards = Vec::with_capacity(unique_scopes.len());
    for scope in unique_scopes {
        guards.push(acquire_scope_lock(
            &locks_root,
            scope,
            workspace_root,
            wait_timeout_ms,
        )?);
    }
    Ok(guards)
}

pub(in crate::runner) fn unlock_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<UnlockResult, RunnerError> {
    let locks_root = paths::ensure_locks_root(workspace_root)?;

    let mut removed = Vec::new();
    let mut missing = Vec::new();
    for scope in scopes {
        let label = scope.label();
        let path = locks_root.join(scope.file_name());
        match fs::remove_file(&path) {
            Ok(()) => removed.push(label),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing.push(label),
            Err(error) => return Err(files::task_lock_io(path.clone(), error)),
        }
    }

    Ok(UnlockResult { removed, missing })
}

pub(in crate::runner) fn unlock_all(workspace_root: &Path) -> Result<UnlockResult, RunnerError> {
    let locks_root = paths::ensure_locks_root(workspace_root)?;

    let mut removed = Vec::new();
    for entry in fs::read_dir(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })? {
        let entry = entry.map_err(|error| RunnerError::TaskLockIo {
            path: locks_root.clone(),
            error,
        })?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("lock") {
            continue;
        }
        files::remove_lock_file(&path)?;
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            removed.push(name.to_owned());
        }
    }

    Ok(UnlockResult {
        removed,
        missing: Vec::new(),
    })
}

fn acquire_scope_lock(
    locks_root: &Path,
    scope: LockScope,
    workspace_root: &Path,
    wait_timeout_ms: u64,
) -> Result<LockGuard, RunnerError> {
    let path = locks_root.join(scope.file_name());
    let scope_label = scope.label();
    let now = stale::now_epoch_ms();
    let token = new_owner_token();
    let record = LockRecord {
        scope: scope_label.clone(),
        pid: std::process::id(),
        started_at_epoch_ms: now,
        heartbeat_at_epoch_ms: now,
        hostname: stale::lock_hostname(),
        workspace_root: Some(workspace_root.display().to_string()),
        owner_token: Some(token.clone()),
        managed_run_token: managed_run_token_from_env(),
    };
    let body = serde_json::to_vec(&record)
        .map_err(|error| RunnerError::Ui(format!("failed to encode lock record: {error}")))?;
    let wait_started_at = Instant::now();
    let deadline = wait_started_at + Duration::from_millis(wait_timeout_ms);

    loop {
        match publish_new_lock(&path, &body) {
            Ok(true) => return Ok(LockGuard::new(path.clone(), record, token)),
            Ok(false) => {
                let existing = match files::read_lock_record(&path) {
                    Ok(record) => Some(record),
                    // Unparsable: abandoned once old enough, otherwise held.
                    // Reclaim only the judged bytes so a successor's
                    // just-published lock is never unlinked.
                    Err(RunnerError::TaskInvocation(_)) => {
                        if unparsable_lock_is_abandoned(&path) {
                            let judged = fs::read(&path).unwrap_or_default();
                            remove_unparsable_if_unchanged(&path, &judged);
                            continue;
                        }
                        None
                    }
                    Err(RunnerError::TaskLockIo { error, .. })
                        if error.kind() == std::io::ErrorKind::NotFound =>
                    {
                        continue;
                    }
                    Err(other) => return Err(other),
                };
                if let Some(existing_record) = existing.as_ref() {
                    if stale::lock_is_stale(existing_record) {
                        remove_if_unchanged(&path, existing_record);
                        continue;
                    }
                }

                if wait_timeout_ms > 0 {
                    let now = Instant::now();
                    if now < deadline {
                        let remaining = deadline.saturating_duration_since(now);
                        thread::sleep(remaining.min(Duration::from_millis(LOCK_WAIT_POLL_MS)));
                        continue;
                    }
                    return Err(stale::lock_conflict(
                        scope_label,
                        path,
                        workspace_root,
                        existing,
                        Some(stale::LockWaitEvidence {
                            timeout_ms: wait_timeout_ms,
                            waited_ms: elapsed_wait_ms(wait_started_at),
                        }),
                    ));
                }

                return Err(stale::lock_conflict(
                    scope_label,
                    path,
                    workspace_root,
                    existing,
                    None,
                ));
            }
            Err(error) => {
                return Err(files::task_lock_io(path.clone(), error));
            }
        }
    }
}

fn elapsed_wait_ms(started_at: Instant) -> u64 {
    started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// The managed run token this process carries, when it belongs to a
/// managed run's process tree. In-container exec'd processes inherit the
/// token through their rendered command; the run's settle uses it to
/// release records their holders could not drop.
fn managed_run_token_from_env() -> Option<String> {
    std::env::var(effigy_managed::RUN_TOKEN_ENV)
        .ok()
        .filter(|token| !token.is_empty())
}

/// One live-or-stale holder named by the checkout's current task-lock
/// records. `managed_run_token` attributes the record to a managed run.
#[derive(Debug, Clone)]
pub(in crate::runner) struct TaskLockHolder {
    pub scope: String,
    pub pid: u32,
    pub hostname: Option<String>,
    pub managed_run_token: Option<String>,
}

/// Every holder named by the checkout's current task-lock records.
/// Unreadable records are skipped: a lock a reader cannot parse is
/// another path's problem, never evidence of ownership.
pub(in crate::runner) fn task_lock_holders(workspace_root: &Path) -> Vec<TaskLockHolder> {
    let locks_dir = workspace_root.join(LOCKS_DIR);
    let Ok(entries) = fs::read_dir(&locks_dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("lock"))
        .filter_map(|path| {
            let record = files::read_lock_record(&path).ok()?;
            Some(TaskLockHolder {
                scope: record.scope,
                pid: record.pid,
                hostname: record.hostname,
                managed_run_token: record.managed_run_token,
            })
        })
        .collect()
}

/// Holders that still exclude another run. Local zombies have ended; foreign
/// PID namespaces retain the existing heartbeat-lease liveness rule.
pub(in crate::runner) fn live_task_lock_holders(workspace_root: &Path) -> Vec<TaskLockHolder> {
    let Ok(entries) = fs::read_dir(workspace_root.join(LOCKS_DIR)) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            if entry.path().extension().and_then(|ext| ext.to_str()) != Some("lock") {
                return None;
            }
            let record = files::read_lock_record(&entry.path()).ok()?;
            let comparable = record
                .hostname
                .as_deref()
                .is_none_or(|host| lock_hostname().as_deref() == Some(host));
            if comparable {
                if !effigy_managed::runtime::managed_owner_is_running(record.pid) {
                    return None;
                }
            } else if stale::lock_is_stale(&record) {
                return None;
            }
            Some(TaskLockHolder {
                scope: record.scope,
                pid: record.pid,
                hostname: record.hostname,
                managed_run_token: record.managed_run_token,
            })
        })
        .collect()
}

/// Remove every lock record attributed to `run_token`, through the lock
/// owner-token rules: each file is unlinked only if it still holds exactly
/// the record that named the run, so a successor's lock is never touched.
/// Records held by `skip_pid` (the live run's own guards, which drop at
/// return) are left for that guard or for the post-cascade sweep. Returns
/// the released lock scopes.
pub(in crate::runner) fn release_locks_attributed_to_run(
    workspace_root: &Path,
    run_token: &str,
    skip_pid: Option<u32>,
) -> Vec<String> {
    let locks_dir = workspace_root.join(LOCKS_DIR);
    let Ok(entries) = fs::read_dir(&locks_dir) else {
        return Vec::new();
    };
    let mut released = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("lock") {
            continue;
        }
        let Ok(record) = files::read_lock_record(&path) else {
            continue;
        };
        if record.managed_run_token.as_deref() != Some(run_token) {
            continue;
        }
        if skip_pid.is_some_and(|pid| record.pid == pid) {
            continue;
        }
        if files::read_lock_record(&path).is_ok_and(|current| current == record) {
            let _ = fs::remove_file(&path);
            released.push(record.scope);
        }
    }
    released
}

impl LockGuard {
    fn new(path: PathBuf, record: LockRecord, token: String) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel();
        let heartbeat_path = path.clone();
        let heartbeat_token = token.clone();
        let heartbeat_thread = thread::spawn(move || {
            let mut record = record;
            loop {
                match stop_rx.recv_timeout(Duration::from_millis(stale::LOCK_HEARTBEAT_INTERVAL_MS))
                {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        // A lost lock stays lost: never overwrite a successor.
                        if !owns_lock(&heartbeat_path, &heartbeat_token) {
                            return;
                        }
                        record.heartbeat_at_epoch_ms = stale::now_epoch_ms();
                        let Ok(body) = serde_json::to_vec(&record) else {
                            return;
                        };
                        let staged = files::staging_path(&heartbeat_path);
                        if fs::write(&staged, body).is_err() {
                            let _ = fs::remove_file(&staged);
                            return;
                        }
                        // Refuse to replace a successor: a reclaim that landed
                        // after the pre-write check must not be overwritten.
                        if !owns_lock(&heartbeat_path, &heartbeat_token) {
                            let _ = fs::remove_file(&staged);
                            return;
                        }
                        if fs::rename(&staged, &heartbeat_path).is_err() {
                            let _ = fs::remove_file(&staged);
                            return;
                        }
                    }
                }
            }
        });
        Self {
            path,
            token,
            stop_heartbeat: Some(stop_tx),
            heartbeat_thread: Some(heartbeat_thread),
        }
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;

    fn scope() -> LockScope {
        LockScope::Task("ownership".to_owned())
    }

    fn lock_path(root: &Path) -> PathBuf {
        root.join(LOCKS_DIR).join(scope().file_name())
    }

    #[test]
    fn published_lock_is_complete_and_names_its_owner() {
        let root = tempfile::tempdir().expect("tempdir");
        let guard = acquire_scopes(root.path(), &[scope()]).expect("acquire");
        let record = files::read_lock_record(&lock_path(root.path())).expect("complete record");
        assert_eq!(record.owner_token.as_deref(), Some(guard[0].token.as_str()));
        let leftovers = fs::read_dir(root.path().join(LOCKS_DIR))
            .expect("locks dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
            .count();
        assert_eq!(leftovers, 0, "staging files are removed after publishing");
    }

    #[test]
    fn released_guard_never_removes_a_successors_lock() {
        let root = tempfile::tempdir().expect("tempdir");
        let guard = acquire_scopes(root.path(), &[scope()]).expect("acquire");
        let path = lock_path(root.path());
        let mut successor = files::read_lock_record(&path).expect("record");
        successor.owner_token = Some("successor".to_owned());
        fs::write(&path, serde_json::to_vec(&successor).unwrap()).expect("successor takes over");

        drop(guard);

        let survivor = files::read_lock_record(&path).expect("successor lock survives");
        assert_eq!(survivor.owner_token.as_deref(), Some("successor"));
    }

    #[test]
    fn fresh_unparsable_lock_is_held_not_deleted() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = lock_path(root.path());
        fs::create_dir_all(path.parent().unwrap()).expect("locks dir");
        fs::write(&path, b"{\"scope\":").expect("partial record mid-write");

        let error =
            acquire_scopes(root.path(), &[scope()]).expect_err("a fresh partial lock is held");
        assert!(
            matches!(error, RunnerError::TaskLockConflict(_)),
            "{error:?}"
        );
        assert!(
            path.exists(),
            "a possibly in-progress lock must not be deleted"
        );
    }

    #[test]
    fn unparsable_reclaim_never_removes_a_successors_complete_lock() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = lock_path(root.path());
        fs::create_dir_all(path.parent().unwrap()).expect("locks dir");
        // Judge corrupt bytes, then let a successor publish before the
        // reclaim runs: the judged bytes must not unlink the new record.
        let judged = b"{invalid".to_vec();
        let _successor = acquire_scopes(root.path(), &[scope()]).expect("successor publishes");
        assert!(!remove_unparsable_if_unchanged(&path, &judged));
        files::read_lock_record(&path).expect("successor lock survives reclaim");
    }

    #[test]
    fn abandoned_unparsable_lock_is_reclaimed() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = lock_path(root.path());
        fs::create_dir_all(path.parent().unwrap()).expect("locks dir");
        fs::write(&path, b"{invalid").expect("corrupt record");
        let old = std::time::SystemTime::now() - UNPARSABLE_LOCK_GRACE * 2;
        fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|file| file.set_modified(old))
            .expect("age the corrupt record");

        let guard = acquire_scopes(root.path(), &[scope()]).expect("reclaim");
        assert_eq!(guard.len(), 1);
    }
}
