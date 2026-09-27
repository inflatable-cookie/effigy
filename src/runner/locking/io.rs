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

#[derive(Debug)]
pub(in crate::runner) struct LockGuard {
    path: PathBuf,
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
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    let record = LockRecord {
        scope: scope_label.clone(),
        pid: std::process::id(),
        started_at_epoch_ms: now,
        heartbeat_at_epoch_ms: now,
        hostname: stale::lock_hostname(),
        workspace_root: Some(workspace_root.display().to_string()),
    };
    let body = serde_json::to_vec(&record)
        .map_err(|error| RunnerError::Ui(format!("failed to encode lock record: {error}")))?;
    let wait_started_at = Instant::now();
    let deadline = wait_started_at + Duration::from_millis(wait_timeout_ms);

    loop {
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&body)
                    .map_err(|error| files::task_lock_io(path.clone(), error))?;
                return Ok(LockGuard::new(path.clone(), record));
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = match files::read_lock_record(&path) {
                    Ok(record) => Some(record),
                    Err(RunnerError::TaskInvocation(_)) => {
                        let _ = files::remove_lock_file(&path);
                        continue;
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
                        let _ = files::remove_lock_file(&path);
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

impl LockGuard {
    fn new(path: PathBuf, record: LockRecord) -> Self {
        let (stop_tx, stop_rx) = mpsc::channel();
        let heartbeat_path = path.clone();
        let heartbeat_thread = thread::spawn(move || {
            let mut record = record;
            loop {
                match stop_rx.recv_timeout(Duration::from_millis(stale::LOCK_HEARTBEAT_INTERVAL_MS))
                {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    Err(mpsc::RecvTimeoutError::Timeout) => {
                        record.heartbeat_at_epoch_ms = stale::now_epoch_ms();
                        let Ok(body) = serde_json::to_vec(&record) else {
                            return;
                        };
                        if fs::write(&heartbeat_path, body).is_err() {
                            return;
                        }
                    }
                }
            }
        });
        Self {
            path,
            stop_heartbeat: Some(stop_tx),
            heartbeat_thread: Some(heartbeat_thread),
        }
    }
}
