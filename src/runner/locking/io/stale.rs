use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nix::errno::Errno;
use nix::sys::signal;
use nix::unistd::Pid;

use crate::runner::error::RunnerError;

pub(super) const LOCK_HEARTBEAT_INTERVAL_MS: u64 = 5_000;
pub(super) const LOCK_STALE_LEASE_MS: u128 = 20_000;

pub(super) fn pid_is_alive(pid: u32) -> bool {
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

pub(super) fn now_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

pub(super) fn lock_conflict(
    scope_label: String,
    lock_path: PathBuf,
    workspace_root: &Path,
    existing: Option<super::LockRecord>,
) -> RunnerError {
    let (holder_pid, started_at, heartbeat_at, holder_hostname, holder_workspace_root) = existing
        .map(|record| {
            (
                Some(record.pid),
                Some(record.started_at_epoch_ms),
                Some(lock_heartbeat_epoch_ms(&record)),
                record.hostname,
                record.workspace_root,
            )
        })
        .unwrap_or((None, None, None, None, None));

    RunnerError::TaskLockConflict {
        scope: scope_label.clone(),
        lock_path,
        holder_pid,
        holder_started_at_epoch_ms: started_at,
        holder_heartbeat_at_epoch_ms: heartbeat_at,
        holder_hostname,
        holder_workspace_root,
        remediation: format!(
            "Resolve the conflicting run or clear lock manually with `effigy unlock {}` (or `effigy unlock --all`) in {}",
            scope_label,
            workspace_root.display()
        ),
    }
}

pub(super) fn lock_heartbeat_epoch_ms(record: &super::LockRecord) -> u128 {
    if record.heartbeat_at_epoch_ms == 0 {
        return record.started_at_epoch_ms;
    }
    record.heartbeat_at_epoch_ms
}

pub(super) fn lock_is_stale(record: &super::LockRecord) -> bool {
    if !pid_is_alive(record.pid) {
        return true;
    }
    let heartbeat_at = lock_heartbeat_epoch_ms(record);
    now_epoch_ms().saturating_sub(heartbeat_at) > LOCK_STALE_LEASE_MS
}

pub(super) fn lock_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
}
