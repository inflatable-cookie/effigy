use std::ffi::OsString;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
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

/// How long output readers may keep draining after the CLI exits. A normal
/// exit closes its pipes at once, so this only bounds a writer that a
/// backgrounded helper keeps open.
const PIPE_EOF_GRACE: Duration = Duration::from_secs(2);

/// Runs `command` with piped stdin-null, stdout and stderr and returns once the
/// child exits. `Command::output` also waits for every pipe writer to close, so
/// a helper that inherited the pipes could hold a detached CLI call open
/// indefinitely. Here readers drain on their own threads: after exit they get
/// `eof_grace` to finish and the bytes read so far are returned. Readers are
/// never killed, and their drain keeps a still-running writer from hitting a
/// broken pipe. A reader ends when its writer closes.
fn capture_output_after_exit(mut command: Command, eof_grace: Duration) -> std::io::Result<Output> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = spawn_pipe_reader(child.stdout.take());
    let stderr = spawn_pipe_reader(child.stderr.take());
    let status = child.wait()?;
    Ok(Output {
        status,
        stdout: collect_pipe_reader(stdout, eof_grace),
        stderr: collect_pipe_reader(stderr, eof_grace),
    })
}

struct PipeReader {
    bytes: Arc<Mutex<Vec<u8>>>,
    finished: mpsc::Receiver<()>,
}

fn spawn_pipe_reader<R: Read + Send + 'static>(pipe: Option<R>) -> Option<PipeReader> {
    let mut pipe = pipe?;
    let bytes = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&bytes);
    let (finished_tx, finished) = mpsc::channel();
    thread::spawn(move || {
        let mut chunk = [0_u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => sink
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .extend_from_slice(&chunk[..read]),
            }
        }
        let _ = finished_tx.send(());
    });
    Some(PipeReader { bytes, finished })
}

fn collect_pipe_reader(reader: Option<PipeReader>, eof_grace: Duration) -> Vec<u8> {
    let Some(reader) = reader else {
        return Vec::new();
    };
    let _ = reader.finished.recv_timeout(eof_grace);
    let captured = reader.bytes.lock().unwrap_or_else(PoisonError::into_inner);
    captured.clone()
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
    mut child: std::process::Child,
    program: &str,
    args: &[&str],
    label: &str,
    deadline: Instant,
    reported_timeout: Duration,
) -> Result<Output, ContainerExecError> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child
                    .wait_with_output()
                    .map_err(|error| ContainerExecError::Launch {
                        command: format!("{program} {}", args.join(" ")),
                        error,
                    });
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                terminate_child_process_tree(&mut child);
                let output =
                    child
                        .wait_with_output()
                        .map_err(|error| ContainerExecError::Launch {
                            command: format!("{program} {}", args.join(" ")),
                            error,
                        })?;
                return timeout_failure(label, reported_timeout, output);
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

fn timeout_failure(
    label: &str,
    timeout: Duration,
    output: Output,
) -> Result<Output, ContainerExecError> {
    Err(ContainerExecError::Failure {
        command: label.to_owned(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: format!(
            "{}\n[effigy] command timed out after {}s",
            String::from_utf8_lossy(&output.stderr).trim_end(),
            timeout.as_secs()
        )
        .trim()
        .to_owned(),
    })
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

        let root = std::env::temp_dir().join(format!(
            "effigy-containers-timeout-descendants-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("mkdir");
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
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Instant;

    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;

    use super::run_command_capture_os_with_env;

    /// A detached `up` that leaves a backgrounded writer on its stdout/stderr
    /// must return when the CLI exits. The sentinel is a fixture child whose
    /// pid is recorded so the test stops only that process.
    #[test]
    fn detached_capture_returns_after_child_exit_when_grandchild_holds_pipes() {
        let fixture = tempfile::tempdir().expect("fresh capture fixture");
        let program = fixture.path().join("fake-compose");
        let sentinel_pid = fixture.path().join("sentinel.pid");
        fs::write(
            &program,
            format!(
                "#!/bin/sh\nsleep 30 &\necho $! > '{}'\necho 'compose up ok'\necho 'compose warn' >&2\nexit 0\n",
                sentinel_pid.display()
            ),
        )
        .expect("write fake compose");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755))
            .expect("make fake compose executable");

        let started = Instant::now();
        let output = run_command_capture_os_with_env(
            fixture.path(),
            program.to_str().expect("utf-8 fixture path"),
            &[],
            "fake compose up",
            &[],
        );
        let elapsed = started.elapsed();

        let pid = fs::read_to_string(&sentinel_pid).expect("sentinel pid recorded");
        let pid: i32 = pid.trim().parse().expect("sentinel pid is numeric");
        let sentinel = Pid::from_raw(pid);
        let sentinel_alive_after_capture = kill(sentinel, None).is_ok();
        let _ = kill(sentinel, Signal::SIGTERM);

        let output = output.expect("compose exited 0");
        assert!(String::from_utf8_lossy(&output.stdout).contains("compose up ok"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("compose warn"));
        assert!(
            elapsed < std::time::Duration::from_secs(10),
            "detached capture waited {elapsed:?} for a pipe writer after the CLI exited"
        );
        assert!(
            sentinel_alive_after_capture,
            "capture must not stop the writer that still holds the pipes"
        );
    }
}
