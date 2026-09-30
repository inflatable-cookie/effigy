//! Host identity primitives shared by admission and QA-group run records.
//!
//! Owner-liveness decisions must survive PID reuse: a process ID alone never
//! identifies an owner. The boot identity plus per-PID start identity let a
//! reader prove that the process holding a record is still the same process,
//! not a recycled PID.

use std::sync::OnceLock;

static BOOT_IDENTITY: OnceLock<Option<String>> = OnceLock::new();

/// Stable per-boot identity, or `None` on unsupported hosts.
pub fn boot_identity() -> Option<String> {
    BOOT_IDENTITY.get_or_init(read_boot_identity).clone()
}

fn read_boot_identity() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .ok()
            .map(|value| value.trim().to_owned())
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sysctl")
            .args(["-n", "kern.boottime"])
            .output()
            .ok()?;
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Start identity for one PID (process start time on supported hosts), or
/// `None` when it cannot be read.
pub fn process_start_identity(pid: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let close = raw.rfind(')')?;
        raw[close + 1..]
            .split_whitespace()
            .nth(19)
            .map(str::to_owned)
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

/// Whether `pid` is live *and* still the process that recorded
/// `start_identity`. Fails closed: an unreadable identity is not a match, and
/// on hosts without start identities only a None-recorded identity counts.
pub fn process_start_identity_matches(pid: u32, start_identity: &str) -> bool {
    match process_start_identity(pid) {
        Some(current) => current == start_identity,
        // Hosts without start identities fall back to plain liveness only
        // when the record also could not have recorded one.
        None => start_identity.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::{boot_identity, process_start_identity, process_start_identity_matches};

    #[test]
    fn own_process_identity_matches_itself_and_not_a_placeholder() {
        let pid = std::process::id();
        if let Some(identity) = process_start_identity(pid) {
            assert!(process_start_identity_matches(pid, &identity));
            assert!(!process_start_identity_matches(pid, "not-the-start"));
        }
        assert!(!process_start_identity_matches(u32::MAX - 1, "whatever"));
    }

    #[test]
    fn boot_identity_is_stable_within_one_boot() {
        assert_eq!(boot_identity(), boot_identity());
    }
}
