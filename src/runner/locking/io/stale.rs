use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) struct LockWaitEvidence {
    pub timeout_ms: u64,
    pub waited_ms: u64,
}

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

#[cfg(unix)]
fn pid_command_line(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rendered = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if rendered.is_empty() {
        None
    } else {
        Some(rendered)
    }
}

#[cfg(unix)]
fn pid_looks_like_effigy(pid: u32) -> bool {
    pid_command_line(pid).is_some_and(|command| {
        command.contains("effigy")
            || command.contains("cargo run --bin effigy")
            || command.contains("/Cargo.toml --bin effigy")
    })
}

#[cfg(not(unix))]
fn pid_looks_like_effigy(_pid: u32) -> bool {
    true
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
    wait: Option<LockWaitEvidence>,
) -> RunnerError {
    let (
        holder_pid,
        started_at,
        heartbeat_at,
        holder_hostname,
        holder_workspace_root,
        managed_run_token,
    ) = existing
        .map(|record| {
            (
                Some(record.pid),
                Some(record.started_at_epoch_ms),
                Some(lock_heartbeat_epoch_ms(&record)),
                record.hostname,
                record.workspace_root,
                record.managed_run_token,
            )
        })
        .unwrap_or((None, None, None, None, None, None));

    let status_command = scope_label
        .strip_prefix("task:")
        .map(|selector| format!("effigy tasks status {selector}"));
    let unlock_command = format!("effigy tasks unlock {scope_label}");
    let remediation = match (&wait, &status_command) {
        (Some(wait), Some(status_command)) => format!(
            "Waited {}ms (timeout {}ms) for a live owner. Inspect with `{status_command}`, then retry after release, or clear lock manually with `{unlock_command}` (or `effigy tasks unlock --all`) in {}",
            wait.waited_ms,
            wait.timeout_ms,
            workspace_root.display()
        ),
        (Some(wait), None) => format!(
            "Waited {}ms (timeout {}ms) for a live owner. Retry after that owner releases, or clear lock manually with `{unlock_command}` (or `effigy tasks unlock --all`) in {}",
            wait.waited_ms,
            wait.timeout_ms,
            workspace_root.display()
        ),
        (None, _) => format!(
            "Resolve the conflicting run or clear lock manually with `{unlock_command}` (or `effigy tasks unlock --all`) in {}",
            workspace_root.display()
        ),
    };
    let details_json = wait.as_ref().map(|wait| {
        serde_json::json!({
            "schema": "effigy.lock-wait.v1",
            "schema_version": 1,
            "scope": scope_label,
            "lock_path": lock_path.display().to_string(),
            "holder_pid": holder_pid,
            "holder_started_at_epoch_ms": started_at,
            "holder_heartbeat_at_epoch_ms": heartbeat_at,
            "holder_hostname": holder_hostname,
            "holder_workspace_root": holder_workspace_root,
            "holder_managed_run_token": managed_run_token,
            "wait_timeout_ms": wait.timeout_ms,
            "waited_ms": wait.waited_ms,
            "status_command": status_command,
            "unlock_command": unlock_command,
            "next": status_command.as_ref().map(|status_command| {
                vec![
                    format!("inspect the live owner with `{status_command}`"),
                    "retry after that owner releases the lock".to_owned(),
                    format!("or clear the scope with `{unlock_command}`"),
                ]
            }).unwrap_or_else(|| vec![
                "retry after the live owner releases the lock".to_owned(),
                format!("or clear the scope with `{unlock_command}`"),
            ]),
        })
        .to_string()
    });

    RunnerError::TaskLockConflict(Box::new(effigy_core::task_lock::TaskLockConflict {
        scope: scope_label,
        lock_path,
        holder_pid,
        holder_started_at_epoch_ms: started_at,
        holder_heartbeat_at_epoch_ms: heartbeat_at,
        holder_hostname,
        holder_workspace_root,
        wait_timeout_ms: wait.as_ref().map(|wait| wait.timeout_ms),
        waited_ms: wait.as_ref().map(|wait| wait.waited_ms),
        status_command,
        details_json,
        remediation,
    }))
}

pub(super) fn lock_heartbeat_epoch_ms(record: &super::LockRecord) -> u128 {
    if record.heartbeat_at_epoch_ms == 0 {
        return record.started_at_epoch_ms;
    }
    record.heartbeat_at_epoch_ms
}

pub(super) fn lock_is_stale(record: &super::LockRecord) -> bool {
    if pid_liveness_is_comparable(record) {
        if !pid_is_alive(record.pid) {
            return true;
        }
        if !pid_looks_like_effigy(record.pid) {
            return true;
        }
    }
    let heartbeat_at = lock_heartbeat_epoch_ms(record);
    now_epoch_ms().saturating_sub(heartbeat_at) > LOCK_STALE_LEASE_MS
}

/// Whether this host's process table can judge the record's holder.
/// PIDs are only comparable inside the PID namespace that wrote the
/// record: a container-side holder's PID means nothing on the host (it may
/// be dead, or worse, name an unrelated live host process), and a host
/// holder's PID means nothing inside a container. A record annotated with
/// a different hostname - or one whose own namespace cannot be named - is
/// therefore judged by its heartbeat lease alone, which the shared
/// checkout keeps current for a live holder.
fn pid_liveness_is_comparable(record: &super::LockRecord) -> bool {
    let Some(written) = record.hostname.as_deref() else {
        // Unannotated records keep today's behavior.
        return true;
    };
    let local = lock_hostname();
    let Some(local) = local.as_deref() else {
        return false;
    };
    written == local
}

pub(in crate::runner) fn lock_hostname() -> Option<String> {
    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            std::env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        // The kernel hostname is authoritative when no environment variable
        // carries it, so host and container holders are always annotated
        // with comparable names.
        .or_else(kernel_hostname)
}

#[cfg(unix)]
fn kernel_hostname() -> Option<String> {
    let mut buffer = [0u8; 256];
    // SAFETY: the buffer is a valid 256-byte region and libc::gethostname
    // writes at most that many bytes plus a NUL terminator.
    let written = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if written != 0 {
        return None;
    }
    let end = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    std::str::from_utf8(&buffer[..end])
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(not(unix))]
fn kernel_hostname() -> Option<String> {
    None
}
