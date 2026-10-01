#[cfg(target_os = "macos")]
use chrono::{TimeZone, Utc};

/// Return the protocol-specific, PID-reuse-resistant identity for a live PID.
/// Unknown or unsupported identities fail closed.
pub fn canonical_start_identity(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    #[cfg(target_os = "linux")]
    {
        let boot = effigy_process::boot_identity()?;
        if boot.is_empty() {
            return None;
        }
        let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        linux_identity(pid, &boot, &raw)
    }
    #[cfg(target_os = "macos")]
    {
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let result = unsafe {
            libc::proc_pidinfo(
                pid as libc::c_int,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int,
            )
        };
        if result != std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int {
            return None;
        }
        let info = unsafe { info.assume_init() };
        if info.pbi_pid != pid {
            return None;
        }
        Some(format_mac_identity(pid, info.pbi_start_tvsec)?)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = pid;
        None
    }
}

#[cfg(target_os = "linux")]
fn linux_identity(pid: u32, boot: &str, stat: &str) -> Option<String> {
    if boot.is_empty() {
        return None;
    }
    let start_ticks = linux_start_ticks(stat)?;
    Some(format!("{pid}@{boot}:{start_ticks}"))
}

#[cfg(target_os = "linux")]
fn linux_start_ticks(stat: &str) -> Option<&str> {
    let command_end = stat.rfind(')')?;
    let fields = stat[command_end + 1..]
        .split_whitespace()
        .collect::<Vec<_>>();
    // The tail begins at field 3 (state), so field 22 is index 19.
    let start_ticks = *fields.get(19)?;
    start_ticks.parse::<u64>().ok()?;
    Some(start_ticks)
}

#[cfg(target_os = "macos")]
fn format_mac_identity(pid: u32, start_seconds: u64) -> Option<String> {
    let seconds = i64::try_from(start_seconds).ok()?;
    let timestamp = Utc.timestamp_opt(seconds, 0).single()?;
    Some(format!("{pid}@{}", timestamp.format("%Y-%m-%dT%H:%M:%SZ")))
}

/// Exact identity comparison. Unknown identities never match, even for self.
pub fn start_identity_matches(pid: u32, expected: &str) -> bool {
    canonical_start_identity(pid).as_deref() == Some(expected)
}

#[cfg(test)]
mod tests {
    use super::{canonical_start_identity, start_identity_matches};

    #[test]
    fn own_process_identity_matches_and_other_generation_does_not() {
        let pid = std::process::id();
        let current = canonical_start_identity(pid).expect("current process identity");
        assert!(start_identity_matches(pid, &current));
        assert!(!start_identity_matches(pid, "1@unknown:0"));
    }

    #[test]
    fn unknown_pid_never_matches() {
        assert!(!start_identity_matches(u32::MAX, ""));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_identity_includes_boot_id_and_start_ticks() {
        let pid = std::process::id();
        let identity = canonical_start_identity(pid).expect("identity");
        let boot = effigy_process::boot_identity().expect("boot id");
        assert!(identity.starts_with(&format!("{pid}@{boot}:")));
        assert!(identity[identity.rfind(':').unwrap() + 1..]
            .parse::<u64>()
            .is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_identity_is_utc_whole_seconds() {
        let identity = canonical_start_identity(std::process::id()).expect("identity");
        assert!(identity.ends_with('Z'));
        assert!(!identity.contains('.'));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_stat_parser_uses_field_22_after_last_parenthesis() {
        let mut fields = vec!["S"; 19];
        fields.push("987654");
        let fixture = format!("321 (worker (with ) parens)) {}", fields.join(" "));
        assert_eq!(super::linux_start_ticks(&fixture), Some("987654"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_boot_change_changes_protocol_identity() {
        let pid = std::process::id();
        let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap();
        let first = super::linux_identity(pid, "boot-one", &raw).unwrap();
        let after_boot_change = super::linux_identity(pid, "boot-two", &raw).unwrap();
        assert_ne!(first, after_boot_change);
        assert!(super::linux_identity(pid, "", &raw).is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_timestamp_is_stable_utc_without_fraction_or_locale() {
        assert_eq!(
            super::format_mac_identity(42, 1_759_320_000).as_deref(),
            Some("42@2025-10-01T12:00:00Z")
        );
        assert_eq!(
            super::format_mac_identity(42, 1_759_320_000),
            super::format_mac_identity(42, 1_759_320_000)
        );
        assert_ne!(
            super::format_mac_identity(42, 1_759_320_000).as_deref(),
            Some("42@2025-10-01T12:00:00.000Z")
        );
    }
}
