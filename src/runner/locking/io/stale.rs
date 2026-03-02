use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use nix::errno::Errno;
use nix::sys::signal;
use nix::unistd::Pid;

use super::super::super::RunnerError;

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
    let (holder_pid, started_at) = existing
        .map(|record| (Some(record.pid), Some(record.started_at_epoch_ms)))
        .unwrap_or((None, None));

    RunnerError::TaskLockConflict {
        scope: scope_label.clone(),
        lock_path,
        holder_pid,
        holder_started_at_epoch_ms: started_at,
        remediation: format!(
            "Resolve the conflicting run or clear lock manually with `effigy unlock {}` (or `effigy unlock --all`) in {}",
            scope_label,
            workspace_root.display()
        ),
    }
}
