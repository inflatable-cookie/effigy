//! Read one process's command line without a setuid helper.
//!
//! macOS `/bin/ps` is setuid root (`-rwsr-xr-x`), so `execve` of it fails with
//! `EPERM` under any `sandbox-exec` Seatbelt profile, even
//! `(version 1)(allow default)`. Listener inspection and suite reporting only
//! need the command line the kernel already holds for a live PID, and both
//! hosts answer that in-process: `KERN_PROCARGS2` on macOS,
//! `/proc/{pid}/cmdline` on Linux. `ps` stays as the fallback for the PIDs the
//! in-process read cannot answer, such as another user's process seen from
//! outside a sandbox, and for hosts without a kernel interface.

use std::process::Command;

/// Largest command line the kernel returns for one process.
#[cfg(target_os = "macos")]
const MAX_PROCARGS: usize = 1 << 20;

/// The full command line of `pid`, or `None` when nothing describes it.
///
/// The tokens are the ones the kernel holds for the process, so a process that
/// rewrote its `argv[0]` reports the rewritten title: OpenSSH's
/// `setproctitle` writes `ssh: /Users/tom/.colima/_lima/colima-effigy/ssh.sock
/// [mux]`, which is the only text that distinguishes Colima/Lima's own port
/// forwarder from an unrelated host listener.
pub fn process_command_line(pid: u32) -> Option<String> {
    read_command_line(pid).or_else(|| command_line_with_ps(pid))
}

/// The kernel's own answer for `pid`, or `None` where it has no answer.
fn read_command_line(pid: u32) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        read_procargs2(pid)
    }
    #[cfg(target_os = "linux")]
    {
        read_proc_cmdline(pid)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = pid;
        None
    }
}

#[cfg(target_os = "macos")]
fn read_procargs2(pid: u32) -> Option<String> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
    let mut bytes = vec![0u8; MAX_PROCARGS];
    let mut size = bytes.len();
    // SAFETY: three initialized MIB integers, and a writable initialized byte
    // allocation of exactly `size` bytes that lives for the call. A null
    // `newp` with a zero `newlen` makes the read read-only.
    let status = unsafe {
        libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if status != 0 || size > bytes.len() {
        // Another user's process, an exited PID, or an oversized command line:
        // the caller falls back to `ps`.
        return None;
    }
    bytes.truncate(size);
    parse_procargs2(&bytes)
}

/// Parse one XNU `KERN_PROCARGS2` buffer into a command line.
///
/// The layout is a native-endian `argc: i32`, the NUL-terminated executable
/// path, NUL padding out to the first argument boundary, `argc`
/// NUL-terminated `argv` strings, then the environment. `argv[0]` is kept as
/// the process's own title, not replaced with the executable path, and the
/// environment that follows it is never read: the buffer is truncated at the
/// last argument the caller collected.
///
/// `None` means the buffer cannot describe a command line at all: shorter than
/// the `argc` prefix, a negative `argc`, or an executable path with no
/// terminator. A buffer that ends mid-argument is truncated rather than
/// malformed, so the arguments that were complete are reported and no invented
/// one is. A buffer whose `argv` holds no text reports the executable path.
#[cfg(target_os = "macos")]
fn parse_procargs2(bytes: &[u8]) -> Option<String> {
    let argc = i32::from_ne_bytes(bytes.get(..4)?.try_into().ok()?);
    if argc < 0 {
        return None;
    }
    // The executable path is NUL-terminated; padding after it is skipped so the
    // first argument starts at the first non-NUL byte.
    let path_start = 4;
    let path_end = bytes
        .get(path_start..)?
        .iter()
        .position(|byte| *byte == 0)?
        + path_start;
    let exec_path = command_token(&bytes[path_start..path_end])?;
    let mut offset = path_end;
    while bytes.get(offset) == Some(&0) {
        offset += 1;
    }
    let mut tokens = Vec::new();
    for _ in 0..argc {
        let Some(rest) = bytes.get(offset..) else {
            break;
        };
        let Some(end) = rest.iter().position(|byte| *byte == 0) else {
            // Truncated mid-argument: keep the complete tokens only.
            break;
        };
        if let Some(token) = command_token(&rest[..end]) {
            tokens.push(token);
        }
        offset += end + 1;
    }
    if tokens.is_empty() {
        Some(exec_path)
    } else {
        Some(tokens.join(" "))
    }
}

#[cfg(target_os = "linux")]
fn read_proc_cmdline(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let bytes = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    join_command_tokens(&bytes)
}

/// Join NUL-separated command-line bytes into one line of text.
#[cfg(target_os = "linux")]
fn join_command_tokens(bytes: &[u8]) -> Option<String> {
    let tokens = bytes.split(|byte| *byte == 0).collect::<Vec<_>>();
    let tokens = tokens
        .into_iter()
        .filter_map(command_token)
        .collect::<Vec<_>>();
    (!tokens.is_empty()).then(|| tokens.join(" "))
}

/// One argument as command-line text, or `None` when it holds no text.
///
/// A non-UTF-8 argument is rendered lossily rather than dropped: the caller is
/// matching and reporting text, not reconstructing an exact `argv`, and losing
/// the token would hide the listener this inspection exists to name.
fn command_token(bytes: &[u8]) -> Option<String> {
    let token = String::from_utf8_lossy(bytes);
    let token = token.trim();
    (!token.is_empty()).then(|| token.to_owned())
}

/// `ps` as the fallback for a PID the kernel read cannot answer.
///
/// This is a best-effort last resort: where the platform refuses the setuid
/// helper (macOS under `sandbox-exec`) the fallback answers nothing, and the
/// caller decides what an unnamed listener means.
fn command_line_with_ps(pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let rendered = String::from_utf8_lossy(&output.stdout);
    let rendered = rendered.trim();
    (!rendered.is_empty()).then(|| rendered.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build one XNU `KERN_PROCARGS2` buffer: `argc`, the NUL-terminated
    /// executable path, `padding` NUL bytes, `argv`, then the environment.
    #[cfg(target_os = "macos")]
    fn procargs2_buffer(
        argc: i32,
        exec_path: &str,
        argv: &[&str],
        padding: usize,
        environment: &[&str],
    ) -> Vec<u8> {
        let mut bytes = argc.to_ne_bytes().to_vec();
        bytes.extend_from_slice(exec_path.as_bytes());
        bytes.push(0);
        bytes.extend(std::iter::repeat_n(0u8, padding));
        for argument in argv {
            bytes.extend_from_slice(argument.as_bytes());
            bytes.push(0);
        }
        for entry in environment {
            bytes.extend_from_slice(entry.as_bytes());
            bytes.push(0);
        }
        bytes
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_keeps_the_openssh_mux_title_that_names_colima() {
        // The exact title Colima/Lima's ControlMaster rewrites into argv[0]
        // through OpenSSH's setproctitle.
        let title = "ssh: /Users/tom/.colima/_lima/colima-effigy/ssh.sock [mux]";
        let bytes = procargs2_buffer(
            1,
            "/usr/bin/ssh",
            &[title],
            4,
            &["HOME=/Users/tom", "SSH_AUTH_SOCK=/var/run/secret"],
        );
        assert_eq!(parse_procargs2(&bytes).as_deref(), Some(title));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_joins_a_normal_command_line_and_ignores_the_environment() {
        let bytes = procargs2_buffer(
            4,
            "/usr/sbin/lsof",
            &[
                "lsof",
                "-nP",
                "-iTCP:49178",
                "-sTCP:LISTEN",
                "leaked-environment=1",
            ],
            0,
            &["PATH=/usr/bin"],
        );
        // argc bounds the arguments: the environment entry that follows is not
        // part of the command line.
        assert_eq!(
            parse_procargs2(&bytes).as_deref(),
            Some("lsof -nP -iTCP:49178 -sTCP:LISTEN")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_skips_padding_and_empty_arguments() {
        let bytes = procargs2_buffer(3, "/usr/bin/docker", &["docker", "", "compose"], 16, &[]);
        assert_eq!(parse_procargs2(&bytes).as_deref(), Some("docker compose"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_without_arguments_reports_the_executable_path() {
        let bytes = procargs2_buffer(0, "/usr/bin/limactl", &[], 0, &[]);
        assert_eq!(parse_procargs2(&bytes).as_deref(), Some("/usr/bin/limactl"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_rejects_a_negative_argument_count() {
        let bytes = procargs2_buffer(-1, "/usr/bin/ssh", &["ssh"], 0, &[]);
        assert_eq!(parse_procargs2(&bytes), None);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn procargs2_truncation_keeps_only_complete_arguments() {
        let full = procargs2_buffer(
            2,
            "/usr/bin/qemu-system-aarch64",
            &["qemu", "--machine"],
            0,
            &[],
        );
        // Cut mid-way through the second argument: the buffer names one
        // complete argument and a prefix of the next.
        let truncated = &full[..full.len() - 4];
        assert_eq!(parse_procargs2(truncated).as_deref(), Some("qemu"));
        // A buffer shorter than the `argc` prefix, or one whose executable path
        // never ends, describes nothing.
        assert_eq!(parse_procargs2(&[0, 0, 0]), None);
        assert_eq!(parse_procargs2(&[1, 0, 0, 0, b'x']), None);
        assert_eq!(parse_procargs2(&[]), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn proc_cmdline_joins_arguments_and_drops_empties() {
        assert_eq!(
            join_command_tokens(b"colima\x00start\x00\x00--foreground\x00").as_deref(),
            Some("colima start --foreground")
        );
        assert_eq!(join_command_tokens(b""), None);
        assert_eq!(join_command_tokens(b"\x00\x00"), None);
    }

    /// The kernel read answers for this process, so nothing has to consult the
    /// setuid helper for the common case.
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn own_command_line_is_readable_in_process() {
        let command = process_command_line(std::process::id()).expect("own command line");
        assert!(
            !command.trim().is_empty(),
            "own command line is empty: {command}"
        );
    }

    /// A PID that cannot exist answers nothing instead of a fabricated name.
    #[test]
    fn an_absent_pid_has_no_command_line() {
        assert_eq!(process_command_line(0), None);
        assert_eq!(process_command_line(u32::MAX), None);
    }
}
