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

/// Read the per-boot identity without consulting the process-wide cache.
///
/// Verification and cross-process proofs use this to show the value comes from
/// the kernel rather than from an earlier in-process read.
pub fn boot_identity_uncached() -> Option<String> {
    read_boot_identity()
}

fn read_boot_identity() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
    }
    #[cfg(target_os = "macos")]
    {
        read_boot_identity_with(|| run_sysctl("kern.bootsessionuuid"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        None
    }
}

/// Whether `recorded` identifies the current boot session.
///
/// New records persist the stable kernel boot-session identity. Records
/// written before that change persisted the macOS `kern.boottime` timeval
/// string, whose microsecond field is not stable across reads. A legacy value
/// therefore matches only through its seconds component, which is stable
/// within a boot and distinct across a reboot at one-second resolution; the
/// caller still requires the exact process start identity. Unreadable current
/// evidence never matches.
pub fn boot_identity_matches(recorded: &str) -> bool {
    let Some(current) = boot_identity() else {
        return false;
    };
    if recorded == current {
        return true;
    }
    #[cfg(target_os = "macos")]
    {
        legacy_boot_time_matches(recorded)
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Raw macOS `kern.boottime` timeval string.
///
/// Compatibility-only evidence: records written before the stable boot-session
/// identity change persisted this value, whose microsecond field drifts. New
/// records must use [`boot_identity`] instead.
#[cfg(target_os = "macos")]
pub fn legacy_boot_time_identity() -> Option<String> {
    run_sysctl("kern.boottime")
        .and_then(|(success, stdout)| success.then_some(stdout))
        .and_then(|stdout| String::from_utf8(stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// Raw macOS `kern.boottime` timeval string; `None` off macOS.
#[cfg(not(target_os = "macos"))]
pub fn legacy_boot_time_identity() -> Option<String> {
    None
}

#[cfg(target_os = "macos")]
fn run_sysctl(name: &str) -> Option<(bool, Vec<u8>)> {
    let output = std::process::Command::new("sysctl")
        .args(["-n", name])
        .output()
        .ok()?;
    Some((output.status.success(), output.stdout))
}

/// Read the boot-session identity through an injected kernel command.
///
/// The closure models a `sysctl -n kern.bootsessionuuid` call and returns
/// `(success, stdout)`. Missing, failed, empty, non-UTF-8 or malformed output
/// is `None` (unknown), never a guessed identity.
#[cfg(target_os = "macos")]
fn read_boot_identity_with(run: impl FnOnce() -> Option<(bool, Vec<u8>)>) -> Option<String> {
    let (success, stdout) = run()?;
    parse_boot_session_uuid(success, &stdout)
}

/// Validate a `kern.bootsessionuuid` result as a canonical 8-4-4-4-12 UUID.
#[cfg(target_os = "macos")]
fn parse_boot_session_uuid(success: bool, stdout: &[u8]) -> Option<String> {
    if !success {
        return None;
    }
    let value = std::str::from_utf8(stdout).ok()?.trim();
    canonical_uuid(value).then(|| value.to_owned())
}

#[cfg(target_os = "macos")]
fn canonical_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
}

#[cfg(target_os = "macos")]
fn legacy_boot_time_matches(recorded: &str) -> bool {
    let Some(recorded_seconds) = parse_boot_time_seconds(recorded) else {
        return false;
    };
    let Some((true, stdout)) = run_sysctl("kern.boottime") else {
        return false;
    };
    let Ok(current) = String::from_utf8(stdout) else {
        return false;
    };
    parse_boot_time_seconds(&current).is_some_and(|seconds| seconds == recorded_seconds)
}

/// Seconds component of a `kern.boottime` timeval string.
#[cfg(target_os = "macos")]
fn parse_boot_time_seconds(value: &str) -> Option<u64> {
    let (_, rest) = value.split_once("sec")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
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
    use super::{
        boot_identity, boot_identity_uncached, process_start_identity,
        process_start_identity_matches,
    };

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

    #[cfg(target_os = "macos")]
    #[test]
    fn boot_session_uuid_parser_rejects_missing_failed_and_malformed() {
        use super::parse_boot_session_uuid;
        let canonical = "5c42afa5-6f5c-420b-b203-86a715e66c68";
        assert_eq!(
            parse_boot_session_uuid(true, format!("{canonical}\n").as_bytes()),
            Some(canonical.to_owned())
        );
        assert_eq!(
            parse_boot_session_uuid(true, canonical.to_uppercase().as_bytes()),
            Some(canonical.to_uppercase())
        );
        assert_eq!(parse_boot_session_uuid(false, canonical.as_bytes()), None);
        assert_eq!(parse_boot_session_uuid(true, b"\n"), None);
        assert_eq!(parse_boot_session_uuid(true, b"not-a-uuid"), None);
        assert_eq!(
            parse_boot_session_uuid(true, b"{ sec = 1, usec = 2 }"),
            None
        );
        assert_eq!(parse_boot_session_uuid(true, &[0xff, 0xfe]), None);
        // 36 characters but wrong separators is malformed, not a session id.
        assert_eq!(
            parse_boot_session_uuid(true, b"5c42afa56f5c420bb20386a715e66c68----"),
            None
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn boot_session_identity_reader_is_stable_and_ignores_boot_time() {
        use super::read_boot_identity_with;
        // The injected kernel command yields the same session identity on both
        // reads even though the host separately reports a changing
        // `kern.boottime` microsecond field. The session identity must not be
        // re-derived from that drifting value.
        let read = || Some((true, b"5c42afa5-6f5c-420b-b203-86a715e66c68\n".to_vec()));
        let first = read_boot_identity_with(read).expect("first same-session identity");
        let second = read_boot_identity_with(read).expect("second same-session identity");
        assert_eq!(first, second);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn legacy_boot_time_identity_matches_across_microsecond_drift() {
        use super::{boot_identity_matches, legacy_boot_time_identity};
        let raw = legacy_boot_time_identity().expect("kernel kern.boottime");
        let (prefix, _) = raw.split_once("usec").expect("timeval usec field");
        let drifted = format!("{prefix}usec = 999999 }}");
        assert_ne!(raw, drifted);
        assert!(boot_identity_matches(&drifted));
        // The stable session identity exact-matches.
        assert!(boot_identity_matches(
            &boot_identity().expect("boot session identity")
        ));
        // A different boot's seconds is a different generation.
        assert!(!boot_identity_matches("{ sec = 1, usec = 2 }"));
        // Unknown or malformed recorded evidence never matches.
        assert!(!boot_identity_matches("not-a-boot-identity"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn boot_identity_uncached_is_stable_across_separate_processes() {
        const HELPER_ENV: &str = "EFFIGY_TEST_BOOT_IDENTITY_HELPER";
        if std::env::var_os(HELPER_ENV).is_some() {
            println!(
                "BOOT_IDENTITY={}",
                boot_identity_uncached().unwrap_or_default()
            );
            return;
        }
        let executable = std::env::current_exe().expect("test executable");
        let read_uncached = || {
            let output = std::process::Command::new(&executable)
                .args([
                    "--exact",
                    "identity::tests::boot_identity_uncached_is_stable_across_separate_processes",
                    "--nocapture",
                ])
                .env(HELPER_ENV, "1")
                .output()
                .expect("spawn uncached identity helper");
            assert!(
                output.status.success(),
                "uncached identity helper failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout)
                .expect("uncached identity helper is UTF-8")
                .lines()
                .find_map(|line| line.strip_prefix("BOOT_IDENTITY="))
                .expect("uncached identity helper printed its value")
                .trim()
                .to_owned()
        };
        let first = read_uncached();
        let second = read_uncached();
        assert!(!first.is_empty(), "session identity must not be empty");
        assert_eq!(
            first, second,
            "separate processes must read one session identity"
        );
        assert_eq!(
            Some(first),
            boot_identity(),
            "uncached cross-process read must agree with the cached value"
        );
    }
}
