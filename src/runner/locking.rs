use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nix::errno::Errno;
use nix::sys::signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};

use super::RunnerError;

const LOCKS_DIR: &str = ".effigy/locks";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum LockScope {
    Workspace,
    Task(String),
    Profile { task: String, profile: String },
}

impl LockScope {
    pub(super) fn parse(value: &str) -> Option<Self> {
        let raw = value.trim();
        if raw == "workspace" {
            return Some(Self::Workspace);
        }
        if let Some(task) = raw.strip_prefix("task:") {
            let task = task.trim();
            if !task.is_empty() {
                return Some(Self::Task(task.to_owned()));
            }
            return None;
        }
        if let Some(rest) = raw.strip_prefix("profile:") {
            let rest = rest.trim();
            let (task, profile) = rest.split_once('/')?;
            let task = task.trim();
            let profile = profile.trim();
            if task.is_empty() || profile.is_empty() {
                return None;
            }
            return Some(Self::Profile {
                task: task.to_owned(),
                profile: profile.to_owned(),
            });
        }
        None
    }

    pub(super) fn label(&self) -> String {
        match self {
            Self::Workspace => "workspace".to_owned(),
            Self::Task(task) => format!("task:{task}"),
            Self::Profile { task, profile } => format!("profile:{task}/{profile}"),
        }
    }

    fn file_name(&self) -> String {
        format!("{}.lock", sanitize_for_file_name(&self.label()))
    }
}

#[derive(Debug)]
pub(super) struct LockGuard {
    path: PathBuf,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct LockRecord {
    scope: String,
    pid: u32,
    started_at_epoch_ms: u128,
}

pub(super) fn acquire_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<Vec<LockGuard>, RunnerError> {
    let mut unique_scopes = scopes.to_vec();
    unique_scopes.sort();
    unique_scopes.dedup();

    let locks_root = workspace_root.join(LOCKS_DIR);
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;

    let mut guards = Vec::with_capacity(unique_scopes.len());
    for scope in unique_scopes {
        guards.push(acquire_scope_lock(&locks_root, scope, workspace_root)?);
    }
    Ok(guards)
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
                    .map_err(|error| RunnerError::TaskLockIo {
                        path: path.clone(),
                        error,
                    })?;
                return Ok(LockGuard { path: path.clone() });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read_lock_record(&path).ok();
                if let Some(existing_record) = existing.as_ref() {
                    if !pid_is_alive(existing_record.pid) {
                        fs::remove_file(&path).map_err(|remove_error| RunnerError::TaskLockIo {
                            path: path.clone(),
                            error: remove_error,
                        })?;
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
                return Err(RunnerError::TaskLockIo {
                    path: path.clone(),
                    error,
                });
            }
        }
    }
}

pub(super) struct UnlockResult {
    pub(super) removed: Vec<String>,
    pub(super) missing: Vec<String>,
}

pub(super) fn unlock_scopes(
    workspace_root: &Path,
    scopes: &[LockScope],
) -> Result<UnlockResult, RunnerError> {
    let locks_root = workspace_root.join(LOCKS_DIR);
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;

    let mut removed = Vec::new();
    let mut missing = Vec::new();
    for scope in scopes {
        let path = locks_root.join(scope.file_name());
        match fs::remove_file(&path) {
            Ok(()) => removed.push(scope.label()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(scope.label())
            }
            Err(error) => {
                return Err(RunnerError::TaskLockIo {
                    path: path.clone(),
                    error,
                });
            }
        }
    }

    Ok(UnlockResult { removed, missing })
}

pub(super) fn unlock_all(workspace_root: &Path) -> Result<UnlockResult, RunnerError> {
    let locks_root = workspace_root.join(LOCKS_DIR);
    fs::create_dir_all(&locks_root).map_err(|error| RunnerError::TaskLockIo {
        path: locks_root.clone(),
        error,
    })?;

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
        fs::remove_file(&path).map_err(|error| RunnerError::TaskLockIo {
            path: path.clone(),
            error,
        })?;
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            removed.push(name.to_owned());
        }
    }

    Ok(UnlockResult {
        removed,
        missing: Vec::new(),
    })
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

fn sanitize_for_file_name(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect::<String>()
}
