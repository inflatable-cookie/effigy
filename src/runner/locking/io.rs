use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nix::errno::Errno;
use nix::sys::signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};

use super::super::RunnerError;
use super::model::LockScope;

const LOCKS_DIR: &str = ".effigy/locks";

#[derive(Debug)]
pub(in crate::runner) struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(in crate::runner) struct UnlockResult {
    pub(in crate::runner) removed: Vec<String>,
    pub(in crate::runner) missing: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LockRecord {
    scope: String,
    pid: u32,
    started_at_epoch_ms: u128,
}

pub(in crate::runner) fn acquire_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<Vec<LockGuard>, RunnerError> {
    let mut unique_scopes = scopes.to_vec();
    unique_scopes.sort();
    unique_scopes.dedup();

    let locks_root = ensure_locks_root(workspace_root)?;

    let mut guards = Vec::with_capacity(unique_scopes.len());
    for scope in unique_scopes {
        guards.push(acquire_scope_lock(&locks_root, scope, workspace_root)?);
    }
    Ok(guards)
}

pub(in crate::runner) fn unlock_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<UnlockResult, RunnerError> {
    let locks_root = ensure_locks_root(workspace_root)?;

    let mut removed = Vec::new();
    let mut missing = Vec::new();
    for scope in scopes {
        let label = scope.label();
        let path = locks_root.join(scope.file_name());
        match fs::remove_file(&path) {
            Ok(()) => removed.push(label),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => missing.push(label),
            Err(error) => return Err(task_lock_io(path.clone(), error)),
        }
    }

    Ok(UnlockResult { removed, missing })
}

pub(in crate::runner) fn unlock_all(workspace_root: &Path) -> Result<UnlockResult, RunnerError> {
    let locks_root = ensure_locks_root(workspace_root)?;

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
        remove_lock_file(&path)?;
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
) -> Result<LockGuard, RunnerError> {
    let path = locks_root.join(scope.file_name());
    let scope_label = scope.label();
    let record = LockRecord {
        scope: scope_label.clone(),
        pid: std::process::id(),
        started_at_epoch_ms: now_epoch_ms(),
    };
    let body = serde_json::to_vec(&record)
        .map_err(|error| RunnerError::Ui(format!("failed to encode lock record: {error}")))?;

    loop {
        match OpenOptions::new().create_new(true).write(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&body)
                    .map_err(|error| task_lock_io(path.clone(), error))?;
                return Ok(LockGuard { path: path.clone() });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read_lock_record(&path).ok();
                if let Some(existing_record) = existing.as_ref() {
                    if !pid_is_alive(existing_record.pid) {
                        remove_lock_file(&path)?;
                        continue;
                    }
                }

                let (holder_pid, started_at) = existing
                    .map(|record| (Some(record.pid), Some(record.started_at_epoch_ms)))
                    .unwrap_or((None, None));
                return Err(RunnerError::TaskLockConflict {
                    scope: scope_label,
                    lock_path: path,
                    holder_pid,
                    holder_started_at_epoch_ms: started_at,
                    remediation: format!(
                        "Resolve the conflicting run or clear lock manually with `effigy unlock {}` (or `effigy unlock --all`) in {}",
                        record.scope,
                        workspace_root.display()
                    ),
                });
            }
            Err(error) => {
                return Err(task_lock_io(path.clone(), error));
            }
        }
    }
}

fn read_lock_record(path: &Path) -> Result<LockRecord, RunnerError> {
    let body = fs::read(path).map_err(|error| RunnerError::TaskLockIo {
        path: path.to_path_buf(),
        error,
    })?;
    serde_json::from_slice::<LockRecord>(&body).map_err(|error| {
        RunnerError::TaskInvocation(format!(
            "failed to parse lock record {}: {error}",
            path.display()
        ))
    })
}

fn ensure_locks_root(workspace_root: &Path) -> Result<PathBuf, RunnerError> {
    let locks_root = workspace_root.join(LOCKS_DIR);
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;
    Ok(locks_root)
}

fn remove_lock_file(path: &Path) -> Result<(), RunnerError> {
    fs::remove_file(path).map_err(|error| task_lock_io(path.to_path_buf(), error))
}

fn task_lock_io(path: PathBuf, error: std::io::Error) -> RunnerError {
    RunnerError::TaskLockIo { path, error }
}

fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let raw = pid as i32;
    match signal::kill(Pid::from_raw(raw), None) {
        Ok(()) => true,
        Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}
