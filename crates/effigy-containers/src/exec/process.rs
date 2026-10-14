use std::ffi::OsString;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[cfg(unix)]
use nix::sys::signal::{kill, Signal};
#[cfg(unix)]
use nix::unistd::{setpgid, Pid};

use crate::compose::resolve_host_cli_program;

use super::implementation::ContainerExecError;

pub fn run_command_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
) -> Result<Output, ContainerExecError> {
    let args = args.iter().map(OsString::from).collect::<Vec<_>>();
    run_command_capture_os(repo_root, program, &args, label)
}

pub fn run_command_capture_allow_failure(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<Output, ContainerExecError> {
    let resolved_program = resolve_host_cli_program(program);
    Command::new(&resolved_program)
        .current_dir(repo_root)
        .args(args)
        .output()
        .map_err(|error| ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        })
}

pub(super) fn run_command_capture_os(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
) -> Result<Output, ContainerExecError> {
    run_command_capture_os_with_env(repo_root, program, args, label, &[])
}

pub(super) fn run_command_capture_os_with_env(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    let resolved_program = resolve_host_cli_program(program);
    let mut command = Command::new(&resolved_program);
    command.current_dir(repo_root).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let output = capture_output_after_exit(command, PIPE_EOF_GRACE).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", format_args(args)),
            error,
        }
    })?;
    if !output.status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

/// One post-exit drain budget for both streams, without retained reader threads.
const PIPE_EOF_GRACE: Duration = Duration::from_secs(2);
const CAPTURE_LIMIT: usize = 1024 * 1024;

pub(super) fn run_detached_command_with_env(
    repo_root: &Path,
    program: &str,
    args: &[OsString],
    label: &str,
    env: &[(String, OsString)],
) -> Result<Output, ContainerExecError> {
    // A detached service must never inherit an Effigy-owned log pipe. A null
    // sink has no reader lifetime and remains writable after this CLI exits.
    let mut command = Command::new(resolve_host_cli_program(program));
    command
        .current_dir(repo_root)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    let status = command
        .status()
        .map_err(|error| ContainerExecError::Launch {
            command: label.to_owned(),
            error,
        })?;
    if !status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(), code: status.code(), stdout: String::new(),
            stderr: "[effigy] detached backend failed; output was isolated from service writers. Inspect the selected scope with `effigy container status` and `effigy container logs`".to_owned(),
        });
    }
    Ok(Output {
        status,
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

fn capture_output_after_exit(mut command: Command, eof_grace: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    capture_child(command.spawn()?, None, eof_grace)
}

#[cfg(unix)]
fn make_nonblocking(pipe: &impl std::os::fd::AsFd) -> std::io::Result<()> {
    use nix::fcntl::{fcntl, FcntlArg, OFlag};
    let flags = fcntl(pipe, FcntlArg::F_GETFL).map_err(std::io::Error::from)?;
    fcntl(
        pipe,
        FcntlArg::F_SETFL(OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK),
    )
    .map_err(std::io::Error::from)?;
    Ok(())
}

fn drain_pipe(
    pipe: &mut impl Read,
    bytes: &mut Vec<u8>,
    eof: &mut bool,
    overflow: &mut bool,
    retention_limit: usize,
) -> std::io::Result<()> {
    // Limit work per pass as well as retained bytes, so a continuously writing
    // stream cannot starve cancellation or the other stream.
    for _ in 0..32 {
        let mut chunk = [0_u8; 8192];
        match pipe.read(&mut chunk) {
            Ok(0) => {
                *eof = true;
                break;
            }
            Ok(read) => {
                let retained = read.min(retention_limit.saturating_sub(bytes.len()));
                bytes.extend_from_slice(&chunk[..retained]);
                *overflow |= retained < read;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

#[cfg(unix)]
fn capture_child(
    mut child: std::process::Child,
    deadline: Option<Instant>,
    eof_grace: Duration,
) -> std::io::Result<Output> {
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("capture stdout missing"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| std::io::Error::other("capture stderr missing"))?;
    if let Err(error) = make_nonblocking(&stdout).and_then(|()| make_nonblocking(&stderr)) {
        terminate_child_process_tree(&mut child);
        let _ = child.wait();
        return Err(error);
    }
    let retention_limit = if deadline.is_some() {
        CAPTURE_LIMIT
    } else {
        usize::MAX
    };
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let (mut out_eof, mut err_eof, mut overflow) = (false, false, false);
    let mut exited = None;
    let mut drain_deadline = None;
    let mut timed_out = false;
    loop {
        if let Err(error) = drain_pipe(
            &mut stdout,
            &mut out,
            &mut out_eof,
            &mut overflow,
            retention_limit,
        )
        .and_then(|()| {
            drain_pipe(
                &mut stderr,
                &mut err,
                &mut err_eof,
                &mut overflow,
                retention_limit,
            )
        }) {
            if exited.is_none() {
                terminate_child_process_tree(&mut child);
            }
            let _ = child.wait();
            return Err(error);
        }
        if exited.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    exited = Some(status);
                    drain_deadline = Some(Instant::now() + eof_grace);
                }
                Ok(None) if deadline.is_some_and(|deadline| Instant::now() >= deadline) => {
                    terminate_child_process_tree(&mut child);
                    exited = Some(child.wait()?);
                    drain_deadline = Some(Instant::now() + eof_grace);
                    timed_out = true;
                }
                Ok(None) => {}
                Err(error) => {
                    terminate_child_process_tree(&mut child);
                    let _ = child.wait();
                    return Err(error);
                }
            }
        }
        if let Some(status) = exited {
            if out_eof && err_eof {
                if timed_out {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "owned capture child terminated and reaped; stdout/stderr EOF verified",
                    ));
                }
                if overflow {
                    return Err(std::io::Error::other(
                        "capture exceeded the per-stream retention limit; output is unavailable",
                    ));
                }
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
            if drain_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(std::io::Error::new(std::io::ErrorKind::TimedOut, "capture unavailable: child exited but inherited pipe EOF was not verified; writer provenance and settlement unknown"));
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(not(unix))]
fn capture_child(
    mut child: std::process::Child,
    _deadline: Option<Instant>,
    _eof_grace: Duration,
) -> std::io::Result<Output> {
    let _ = child.kill();
    let _ = child.wait();
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "bounded container capture requires a nonblocking pipe adapter on this platform",
    ))
}

pub(super) fn format_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn run_command_capture_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<Output, ContainerExecError> {
    // The timeout origin is fixed before the spawn, so a slow fork/exec cannot
    // extend the budget the caller asked for.
    let deadline = Instant::now() + timeout;
    let output = spawn_and_supervise_capture(repo_root, program, args, label, deadline, timeout)?;
    require_success(label, output)
}

/// Absolute-deadline capture. The caller's monotonic deadline is supervised
/// unchanged: the origin is never recomputed after spawn, so a slow spawn
/// cannot rebase it. When spawn returns past the deadline the owned child is
/// terminated and reaped immediately rather than being granted fresh time.
pub(super) fn run_command_capture_with_absolute_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let output =
        spawn_and_supervise_capture(repo_root, program, args, label, deadline, reported_timeout)?;
    require_success(label, output)
}

/// Build the canonical timeout failure without spawning a child. A shared
/// monotonic deadline that has already elapsed must never spawn the
/// subprocess; callers see the same "timed out" verdict they get after a
/// real timeout, and never a false success.
pub(super) fn deadline_already_elapsed(label: &str) -> ContainerExecError {
    timeout_error(label)
}

/// Resolve the remaining slice of a shared monotonic deadline, failing closed
/// when it has nothing left. Each probe converts the deadline exactly once,
/// immediately before it spawns, so sibling probes share one budget instead of
/// receiving a fresh per-subprocess timeout.
pub(super) fn remaining_until_deadline(
    deadline: Instant,
    label: &str,
) -> Result<Duration, ContainerExecError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(deadline_already_elapsed(label));
    }
    Ok(remaining)
}

/// Deadline-aware variant of [`run_command_capture_allow_failure`]. With no
/// deadline it keeps the unbounded production behavior; with one the same
/// absolute deadline supervises the spawn, so it is never rebased after spawn.
pub(super) fn run_command_capture_allow_failure_with_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Option<Instant>,
) -> Result<Output, ContainerExecError> {
    let Some(deadline) = deadline else {
        return run_command_capture_allow_failure(repo_root, program, args);
    };
    let reported_timeout = remaining_until_deadline(deadline, label)?;
    run_command_capture_allow_failure_with_absolute_deadline(
        repo_root,
        program,
        args,
        label,
        deadline,
        reported_timeout,
    )
}

pub(super) fn run_command_capture_allow_failure_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let deadline = Instant::now() + timeout;
    spawn_and_supervise_capture(repo_root, program, args, label, deadline, timeout)
}

/// Absolute-deadline variant of
/// [`run_command_capture_allow_failure_with_timeout`] that keeps the caller's
/// deadline as the single supervision origin.
pub(super) fn run_command_capture_allow_failure_with_absolute_deadline(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    spawn_and_supervise_capture(repo_root, program, args, label, deadline, reported_timeout)
}

/// Spawn once and supervise the child against an already-resolved absolute
/// `deadline`. The deadline is never re-derived from the post-spawn clock, so
/// every caller on this path shares the same origin.
fn spawn_and_supervise_capture(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    let child = spawn_capture_child(repo_root, program, args).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }
    })?;
    supervise_capture_child(child, program, args, label, deadline, reported_timeout)
}

fn supervise_capture_child(
    child: std::process::Child,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    match capture_child(child, Some(deadline), PIPE_EOF_GRACE) {
        Ok(output) => Ok(output),
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {
            Err(ContainerExecError::Failure {
                command: label.to_owned(),
                code: None,
                stdout: String::new(),
                stderr: format!(
                    "[effigy] command timed out after {}s: {error}",
                    reported_timeout.as_secs()
                ),
            })
        }
        Err(error) => Err(ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }),
    }
}

fn require_success(label: &str, output: Output) -> Result<Output, ContainerExecError> {
    if !output.status.success() {
        return Err(ContainerExecError::Failure {
            command: label.to_owned(),
            code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }
    Ok(output)
}

fn timeout_error(label: &str) -> ContainerExecError {
    ContainerExecError::Failure {
        command: label.to_owned(),
        code: None,
        stdout: String::new(),
        stderr: "[effigy] command timed out".to_owned(),
    }
}

#[cfg(test)]
fn run_command_stream_with_timeout(
    repo_root: &Path,
    program: &str,
    args: &[&str],
    label: &str,
    timeout: Duration,
) -> Result<(), ContainerExecError> {
    let mut child = spawn_stream_child(repo_root, program, args).map_err(|error| {
        ContainerExecError::Launch {
            command: format!("{program} {}", args.join(" ")),
            error,
        }
    })?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` failed after streaming output directly to the terminal"
                    ),
                });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_tree(&mut child);
                let status = child.wait().map_err(|error| ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                })?;
                return Err(ContainerExecError::Failure {
                    command: label.to_owned(),
                    code: status.code(),
                    stdout: String::new(),
                    stderr: format!(
                        "[effigy] `{label}` timed out after {}s while streaming output directly to the terminal",
                        timeout.as_secs()
                    ),
                });
            }
            Err(error) => {
                terminate_child_process_tree(&mut child);
                let _ = child.wait();
                return Err(ContainerExecError::Launch {
                    command: format!("{program} {}", args.join(" ")),
                    error,
                });
            }
        }
    }
}

/// Test-only seam for a controlled spawn delay. Kept thread-local so it never
/// leaks into another test thread, and always absent in production builds.
#[cfg(test)]
mod test_spawn_delay {
    use std::cell::Cell;
    use std::time::Duration;

    thread_local! {
        static DELAY: Cell<Option<Duration>> = const { Cell::new(None) };
    }

    pub(super) fn get() -> Option<Duration> {
        DELAY.with(Cell::get)
    }

    fn set(delay: Option<Duration>) {
        DELAY.with(|cell| cell.set(delay));
    }

    /// RAII guard that clears the seam on drop, even after a panic.
    pub(super) struct SpawnDelayGuard;

    impl SpawnDelayGuard {
        pub(super) fn set(delay: Duration) -> Self {
            set(Some(delay));
            Self
        }
    }

    impl Drop for SpawnDelayGuard {
        fn drop(&mut self) {
            set(None);
        }
    }
}

fn spawn_capture_child(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    // Private timing seam: simulates a slow fork/exec at the spawn boundary so
    // a test can prove the supervision deadline is not rebased after spawn.
    // Always zero in production and unset in every other test thread.
    #[cfg(test)]
    if let Some(delay) = test_spawn_delay::get() {
        thread::sleep(delay);
    }
    let resolved_program = resolve_host_cli_program(program);
    let mut command = Command::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs this closure in the forked child before `exec`,
    // where only async-signal-safe work is allowed. `setpgid` is
    // async-signal-safe and places the child in a new process group whose id
    // is the child's own pid; the error path uses the allocation-free
    // `io::Error::from` conversion. `terminate_child_process_tree` signals
    // `kill(-pid, ...)` for that same group, so timeout cleanup owns the child
    // and every descendant still in its group.
    unsafe {
        command
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    command.spawn()
}

#[cfg(test)]
fn spawn_stream_child(
    repo_root: &Path,
    program: &str,
    args: &[&str],
) -> Result<std::process::Child, std::io::Error> {
    let resolved_program = resolve_host_cli_program(program);
    let mut command = Command::new(&resolved_program);
    command
        .current_dir(repo_root)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .stdin(Stdio::null());
    #[cfg(unix)]
    // SAFETY: same contract as `spawn_capture_child`: `pre_exec` runs before
    // `exec` where only async-signal-safe calls are permitted, `setpgid` is
    // async-signal-safe, the error conversion allocates nothing, and the new
    // child-owned process group is what `terminate_child_process_tree` targets
    // with `kill(-pid, ...)`.
    unsafe {
        command
            .pre_exec(|| setpgid(Pid::from_raw(0), Pid::from_raw(0)).map_err(std::io::Error::from));
    }
    command.spawn()
}

fn terminate_child_process_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let pid = child.id() as i32;
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_millis(800);
        while Instant::now() < deadline {
            if child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(40));
        }
        if pid > 0 {
            let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
        }
    }
    #[cfg(not(unix))]
    {
        let _ = child.kill();
    }
}

pub(super) fn error_is_timeout(error: &ContainerExecError) -> bool {
    match error {
        ContainerExecError::Failure { stderr, .. } => stderr.contains("command timed out"),
        ContainerExecError::Launch { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_timeout_message_mentions_elapsed_seconds() {
        let error = run_command_capture_with_timeout(
            Path::new("."),
            "/bin/sh",
            &["-c", "/bin/sleep 2"],
            "sleep test",
            Duration::from_millis(200),
        )
        .expect_err("sleep should time out");

        match error {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "sleep test");
                assert!(stderr.contains("command timed out"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn allow_failure_timeout_keeps_nonzero_exit_as_output() {
        let output = run_command_capture_allow_failure_with_timeout(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf 'warn\\n' >&2; exit 7"],
            "start test",
            Duration::from_secs(2),
        )
        .expect("nonzero start should be captured");
        assert_eq!(output.status.code(), Some(7));
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("warn"),
            "got: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn allow_failure_timeout_reports_elapsed_seconds() {
        let error = run_command_capture_allow_failure_with_timeout(
            Path::new("."),
            "/bin/sh",
            &["-c", "/bin/sleep 2"],
            "start hang",
            Duration::from_millis(200),
        )
        .expect_err("sleep should time out");

        match error {
            ContainerExecError::Failure {
                command, stderr, ..
            } => {
                assert_eq!(command, "start hang");
                assert!(stderr.contains("command timed out"), "got: {stderr}");
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn streamed_command_failure_reports_streaming_footer() {
        let error = run_command_stream_with_timeout(
            Path::new("."),
            "/bin/sh",
            &[
                "-c",
                "echo streamed-output; echo streamed-error >&2; exit 7",
            ],
            "stream test",
            Duration::from_secs(1),
        )
        .expect_err("command should fail");

        match error {
            ContainerExecError::Failure {
                command,
                code,
                stderr,
                ..
            } => {
                assert_eq!(command, "stream test");
                assert_eq!(code, Some(7));
                assert!(stderr.contains("streaming output directly to the terminal"));
            }
            other => panic!("expected failure, got {other:?}"),
        }
    }

    #[test]
    fn timeout_detection_matches_timeout_footer() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: None,
            stdout: String::new(),
            stderr: "[effigy] command timed out after 45s".to_owned(),
        };
        assert!(error_is_timeout(&error));
    }

    #[test]
    fn timeout_detection_ignores_non_timeout_failures() {
        let error = ContainerExecError::Failure {
            command: "colima stop".to_owned(),
            code: Some(1),
            stdout: String::new(),
            stderr: "plain failure".to_owned(),
        };
        assert!(!error_is_timeout(&error));
    }

    #[cfg(unix)]
    #[test]
    fn timeout_terminates_background_descendants() {
        use nix::sys::signal::kill;
        use nix::unistd::Pid;
        use std::fs;

        let fixture = tempfile::tempdir().expect("fresh descendant fixture");
        let root = fixture.path();
        let pid_file = root.join("descendant.pid");
        let script = format!(
            "/bin/sh -c 'echo $$ > \"{}\"; trap \"exit 0\" TERM; /bin/sleep 5' & wait",
            pid_file.display()
        );

        let error = run_command_capture_with_timeout(
            &root,
            "/bin/sh",
            &["-c", script.as_str()],
            "descendant test",
            Duration::from_millis(200),
        )
        .expect_err("script should time out");

        match error {
            ContainerExecError::Failure { command, .. } => {
                assert_eq!(command, "descendant test");
            }
            other => panic!("expected failure, got {other:?}"),
        }

        let descendant_pid = fs::read_to_string(&pid_file)
            .expect("pid file")
            .trim()
            .parse::<i32>()
            .expect("pid");
        thread::sleep(Duration::from_millis(150));
        assert!(
            kill(Pid::from_raw(descendant_pid), None).is_err(),
            "expected descendant pid {descendant_pid} to be gone"
        );
    }

    // ------------------------------------------------------------------
    // Deadline continuity (task effigy#093). A caller's absolute deadline is
    // the single supervision origin: a slow spawn cannot rebase it, an expired
    // deadline never spawns, and a child that returns after the deadline is
    // promptly terminated and reaped. The spawn delay is a private thread-local
    // seam; real process supervision is retained throughout.
    // ------------------------------------------------------------------

    #[cfg(unix)]
    const DEADLINE_MARKER_SCRIPT: &str =
        "PATH=; export PATH; case \"$PATH\" in '') : > spawned ;; *) exit 97 ;; esac";

    fn deadline_fixture_root(label: &str) -> std::path::PathBuf {
        use std::fs;
        let root = std::env::temp_dir().join(format!(
            "effigy-containers-deadline-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir deadline fixture");
        root
    }

    #[cfg(unix)]
    #[test]
    fn absolute_deadline_returns_ordinary_output_when_child_finishes_in_time() {
        let output = run_command_capture_allow_failure_with_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf parsed-ok"],
            "ordinary absolute probe",
            Some(Instant::now() + Duration::from_secs(5)),
        )
        .expect("in-time child must succeed");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "parsed-ok");
    }

    #[cfg(unix)]
    #[test]
    fn no_deadline_keeps_the_unbounded_production_path() {
        let output = run_command_capture_allow_failure_with_deadline(
            Path::new("."),
            "/bin/sh",
            &["-c", "printf none-ok; exit 3"],
            "none compatibility",
            None,
        )
        .expect("no deadline keeps allow-failure semantics");
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(String::from_utf8_lossy(&output.stdout), "none-ok");
    }

    #[cfg(unix)]
    #[test]
    fn deadline_marker_control_expired_deadline_never_spawns() {
        let root = deadline_fixture_root("expired");
        let marker = root.join("spawned");
        let expired = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("expired instant");

        let error = run_command_capture_allow_failure_with_deadline(
            &root,
            "/bin/sh",
            &["-c", DEADLINE_MARKER_SCRIPT],
            "expired absolute probe",
            Some(expired),
        )
        .expect_err("expired deadline must fail closed");

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "an expired absolute deadline must never spawn the child"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Negative control for the expired-no-spawn oracle: without a caller
    /// deadline the same fixture does spawn and write its marker, so the
    /// no-spawn oracle is non-vacuous.
    #[cfg(unix)]
    #[test]
    fn deadline_marker_control_no_deadline_still_spawns() {
        let root = deadline_fixture_root("expired-control");
        let marker = root.join("spawned");

        let output = run_command_capture_allow_failure_with_deadline(
            &root,
            "/bin/sh",
            &["-c", DEADLINE_MARKER_SCRIPT],
            "unbounded negative control",
            None,
        )
        .expect("no deadline keeps the unbounded path");

        assert!(output.status.success(), "got {:?}", output.status);
        assert!(
            marker.exists(),
            "without a deadline the child spawns, so the no-spawn oracle is non-vacuous"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[cfg(unix)]
    #[test]
    fn slow_spawn_cannot_rebase_the_absolute_deadline() {
        let root = deadline_fixture_root("no-rebase");
        let marker = root.join("child-finished");
        let script = format!("sleep 1; printf done > '{}'", marker.display());
        let deadline = Instant::now() + Duration::from_secs(2);
        let guard = test_spawn_delay::SpawnDelayGuard::set(Duration::from_secs(3));

        let error = run_command_capture_allow_failure_with_deadline(
            &root,
            "/bin/sh",
            &["-c", script.as_str()],
            "slow spawn absolute probe",
            Some(deadline),
        )
        .expect_err("a deadline already past at spawn must fail closed");
        drop(guard);

        assert!(error.to_string().contains("timed out"), "got {error}");
        assert!(
            !marker.exists(),
            "the origin must not be reset after spawn: the child must be reaped before it finishes"
        );
    }

    /// Negative control for the no-rebase oracle: manually reproducing the old
    /// post-spawn rebasing lets the same child finish and write its marker, so
    /// the positive oracle is non-vacuous. The private guard reaps the child.
    #[cfg(unix)]
    #[test]
    fn rebased_deadline_negative_control_lets_the_short_child_finish() {
        use std::fs;
        let root = deadline_fixture_root("rebase-control");
        let marker = root.join("child-finished");
        let script = format!("sleep 1; printf done > '{}'", marker.display());
        let remaining = Duration::from_secs(2);

        // Simulated slow spawn, then the old `now + remaining` origin reset.
        thread::sleep(Duration::from_secs(3));
        let child = spawn_capture_child(&root, "/bin/sh", &["-c", script.as_str()])
            .expect("spawn rebase control child");
        let rebased_deadline = Instant::now() + remaining;
        let output = supervise_capture_child(
            child,
            "/bin/sh",
            &["-c", script.as_str()],
            "rebased negative control",
            rebased_deadline,
            remaining,
        )
        .expect("rebased supervision must let the child finish");

        assert!(output.status.success(), "got {:?}", output.status);
        assert!(
            marker.exists(),
            "with a rebased origin the short child completes, so the no-rebase oracle is non-vacuous"
        );
        let _ = fs::remove_dir_all(&root);
    }
}

#[cfg(all(test, unix))]
mod detached_capture_tests {
    use super::{run_command_capture_os_with_env, run_command_capture_with_timeout};
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    struct Sentinel(std::process::Child);
    impl Drop for Sentinel {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn detached_null_sink_and_inventory_cancel_preserve_independent_pipe_writer() {
        let fixture = tempfile::tempdir().expect("fresh pipe ownership fixture");
        let sentinel_program = fixture.path().join("sentinel.py");
        fs::write(&sentinel_program, r#"import os,sys,json,stat
json.dump({'pid':os.getpid(),'group':os.getpgrp(),'stdout_inode':os.fstat(1).st_ino,'stdout_fifo':stat.S_ISFIFO(os.fstat(1).st_mode)},open(sys.argv[1],'w'))
for line in sys.stdin:
 print('sentinel:'+line.strip(),flush=True)
"#).expect("private writer program");
        let mut sentinel = Sentinel(
            Command::new("python3")
                .arg("-u")
                .arg(&sentinel_program)
                .arg(fixture.path().join("sentinel.json"))
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .expect("own independent writer"),
        );
        let mut reader = BufReader::new(sentinel.0.stdout.take().expect("sentinel read endpoint"));
        fn probe(
            sentinel: &mut Sentinel,
            reader: &mut BufReader<std::process::ChildStdout>,
            message: &str,
        ) {
            writeln!(
                sentinel.0.stdin.as_mut().expect("owned control"),
                "{message}"
            )
            .expect("send control");
            let mut response = String::new();
            reader
                .read_line(&mut response)
                .expect("writer acknowledgement");
            assert_eq!(response.trim(), format!("sentinel:{message}"));
            assert!(sentinel.0.try_wait().expect("owned status").is_none());
        }
        probe(&mut sentinel, &mut reader, "before");
        let compose = fixture.path().join("compose.py");
        fs::write(&compose, r#"import os,json,stat,sys
json.dump({'pid':os.getpid(),'stdout_null':os.fstat(1).st_rdev==os.stat(os.devnull).st_rdev and stat.S_ISCHR(os.fstat(1).st_mode),'stderr_null':os.fstat(2).st_rdev==os.stat(os.devnull).st_rdev and stat.S_ISCHR(os.fstat(2).st_mode)},open('compose-identities.json','w'))
print('detached output')
print('detached diagnostic',file=sys.stderr)
"#).expect("private detached CLI");
        let mut policy = crate::with_test_effigy_home(&fixture.path().join("home"), || {
            fs::write(fixture.path().join("effigy.toml"),"[containers]\ndefault = \"web\"\n[containers.web]\ncompose_file = \"compose.yml\"\nprimary_service = \"app\"\n").expect("manifest");
            fs::write(
                fixture.path().join("compose.yml"),
                "services:\n  app:\n    image: fixture.invalid/app\n",
            )
            .expect("compose");
            crate::load_container_policy(fixture.path(), None).expect("private policy")
        });
        policy.shared_services.clear();
        let args = [compose.into_os_string(), "up".into(), "-d".into()];
        let output = super::super::implementation::run_compose_invocation_capture_with_env(
            fixture.path(),
            &policy,
            std::ffi::OsStr::new("python3"),
            &args,
            "private detached up",
            &[],
        )
        .expect("production detached adapter completes");
        assert!(output.stdout.is_empty() && output.stderr.is_empty());
        let identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("compose-identities.json")).expect("actual FD receipt"),
        )
        .expect("identity JSON");
        assert_eq!(identity["stdout_null"], true);
        assert_eq!(identity["stderr_null"], true);
        let inventory = fixture.path().join("inventory.py");
        fs::write(&inventory,r#"import os,json,time,stat
json.dump({'pid':os.getpid(),'group':os.getpgrp(),'stdout_fifo':stat.S_ISFIFO(os.fstat(1).st_mode),'stdout_inode':os.fstat(1).st_ino},open('inventory-identities.json','w'))
while True:
 os.write(1,b'x'*8192)
 os.write(2,b'y'*8192)
"#).expect("private hung inventory");
        let started = Instant::now();
        let failure = run_command_capture_with_timeout(
            fixture.path(),
            "python3",
            &[inventory.to_str().expect("private path")],
            "private inventory",
            Duration::from_millis(500),
        )
        .expect_err("bounded inventory refuses");
        assert!(
            failure
                .to_string()
                .contains("terminated and reaped; stdout/stderr EOF verified"),
            "{failure}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        probe(&mut sentinel, &mut reader, "after-inventory");
        let inventory_identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("inventory-identities.json"))
                .expect("inventory FD receipt"),
        )
        .expect("JSON");
        let sentinel_identity: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.path().join("sentinel.json")).expect("sentinel FD receipt"),
        )
        .expect("JSON");
        assert_eq!(inventory_identity["pid"], inventory_identity["group"]);
        assert_ne!(inventory_identity["group"], sentinel_identity["group"]);
        assert_eq!(inventory_identity["stdout_fifo"], true);
        assert_ne!(
            inventory_identity["stdout_inode"],
            sentinel_identity["stdout_inode"]
        );
        assert_eq!(sentinel_identity["stdout_fifo"], true);
        println!("actual detached FD={identity}; inventory={inventory_identity}; independent writer={sentinel_identity}; {failure}");
        drop(sentinel.0.stdin.take());
        assert!(sentinel
            .0
            .wait()
            .expect("positively reap independent writer")
            .success());
    }

    #[test]
    fn bounded_capture_distinguishes_eof_from_child_exit_and_refuses_overflow() {
        let fixture = tempfile::tempdir().expect("fresh capture fixture");
        let started = Instant::now();
        let output = run_command_capture_os_with_env(
            fixture.path(),
            "python3",
            &[
                "-c".into(),
                "import os,time; os.close(1); os.close(2); time.sleep(0.3)".into(),
            ],
            "EOF before exit",
            &[],
        )
        .expect("child settled");
        assert!(output.status.success());
        assert!(started.elapsed() >= Duration::from_millis(250));
        let error = run_command_capture_with_timeout(
            fixture.path(),
            "python3",
            &["-c", "import os; os.write(1,b'x'*(1024*1024+8192))"],
            "overflow",
            Duration::from_secs(5),
        )
        .expect_err("overflow never success");
        assert!(error.to_string().contains("retention limit"));
    }
}
