//! Caller-bounded finite command capture.
//!
//! This is the opt-in bounded alternative to
//! [`super::transport::run_command_capture_until`]. It enforces one caller
//! absolute deadline and independent stdout/stderr retention budgets *while
//! reading*, never routes either stream through an unbounded
//! `Command::output()`/`read_to_end`, and terminates and reaps only the exact
//! process group it created.
//!
//! Contract boundaries:
//!
//! - Retention is bounded per stream before collection. A stream that produces
//!   more than its budget is an error even when the client exits `0`; the
//!   retained prefix is never reported as success.
//! - One absolute [`Instant`] deadline covers spawn, client exit, pipe
//!   draining and owned-group termination. Draining does not rebase it.
//! - Both streams drain concurrently into bounded buffers so asymmetric
//!   flooding cannot deadlock the child or the reader.
//! - A child that exits while an inherited pipe writer keeps the pipe open
//!   cannot make the caller wait forever: the deadline bounds the final drain.
//! - Terminating the owned host client group proves nothing about a remote
//!   container process. Remote cancellation stays explicitly unverified; this
//!   module never guesses a remote PID and never repeats a command.
//!
//! Only a process group Effigy created for this spawn is signaled. The group id
//! is the child pid recorded at spawn (`setpgid(0, 0)` in `pre_exec`), so the
//! negative-pid signal reaches exactly that group.

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command as ProcessCommand, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use effigy_containers::ContainerComposeInvocationPlan;

use super::transport::resolve_host_program;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

/// Read chunk. The retention check runs on whole chunks, so a budget is never
/// silently exceeded by the reader's own buffering.
const BOUNDED_READ_CHUNK: usize = 16 * 1024;
/// How often the monitor loop observes child exit, overflow and the deadline.
const BOUNDED_POLL_INTERVAL: Duration = Duration::from_millis(5);
/// Owned-group SIGTERM-to-SIGKILL grace. Never folded into the command
/// deadline; it is a fixed, documented cleanup bound.
const BOUNDED_TERM_GRACE: Duration = Duration::from_millis(500);

/// One command's retained output plus its exit status.
#[derive(Debug)]
pub(in crate::runner) struct BoundedCapture {
    pub(in crate::runner) status: ExitStatus,
    pub(in crate::runner) stdout: Vec<u8>,
    pub(in crate::runner) stderr: Vec<u8>,
}

impl BoundedCapture {
    /// Clone the capture as a standard [`Output`] for the existing
    /// workspace-user fallback detector without consuming the capture.
    pub(in crate::runner) fn as_output(&self) -> Output {
        Output {
            status: self.status,
            stdout: self.stdout.clone(),
            stderr: self.stderr.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedStream {
    Stdout,
    Stderr,
}

impl BoundedStream {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}
/// Why a bounded capture could not produce a complete, in-budget result.
#[derive(Debug)]
pub(in crate::runner) enum BoundedCaptureFailure {
    /// The caller's absolute deadline expired before a complete result.
    Deadline,
    /// A stream produced more than its remaining retention budget. The
    /// retained prefix is not a success.
    Overflow {
        stream: BoundedStream,
        limit_bytes: u64,
        retained_bytes: u64,
    },
    /// A stream pipe could not be read. The retained prefix is incomplete.
    ReadFailure { stream: BoundedStream },
    /// The reader finished without a complete outcome for a stream.
    IncompleteOutput { stream: BoundedStream },
    /// The command could not be launched at all.
    Launch { label: String, error: String },
}

/// One attempt's absolute deadline and per-stream remaining budgets.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct BoundedStreamLimit {
    pub(in crate::runner) deadline: Instant,
    pub(in crate::runner) stdout_bytes: u64,
    pub(in crate::runner) stderr_bytes: u64,
}

/// Cumulative retention budget for one bounded container operation.
///
/// A single absolute deadline and a single pair of stream budgets live here.
/// Every captured command in the operation — including any eligible existing
/// identity fallback — draws from the same remaining budget, so the hard total
/// cannot be reset by a second attempt.
#[derive(Debug)]
pub(in crate::runner) struct BoundedCommandBudget {
    deadline: Instant,
    stdout_remaining: u64,
    stderr_remaining: u64,
    stdout_retained: u64,
    stderr_retained: u64,
    attempts: u32,
}

impl BoundedCommandBudget {
    pub(in crate::runner) fn new(deadline: Instant, stdout_bytes: u64, stderr_bytes: u64) -> Self {
        Self {
            deadline,
            stdout_remaining: stdout_bytes,
            stderr_remaining: stderr_bytes,
            stdout_retained: 0,
            stderr_retained: 0,
            attempts: 0,
        }
    }

    pub(in crate::runner) fn stdout_retained(&self) -> u64 {
        self.stdout_retained
    }

    pub(in crate::runner) fn stderr_retained(&self) -> u64 {
        self.stderr_retained
    }

    pub(in crate::runner) fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Run one resolved compose plan within the remaining budget and account
    /// its retained bytes cumulatively.
    ///
    /// The plan program and argv are executed exactly as resolved. The bounded
    /// opt-in path deliberately does not take the Colima direct-exec branch:
    /// that branch performs a preparatory `compose ps` capture, while here
    /// every command-output path in the operation is this bounded reader.
    pub(in crate::runner) fn run_plan(
        &mut self,
        plan: &ContainerComposeInvocationPlan,
    ) -> Result<BoundedCapture, BoundedCaptureFailure> {
        // An expired deadline refuses before any spawn, so it is not counted as
        // an attempt.
        if Instant::now() >= self.deadline {
            return Err(BoundedCaptureFailure::Deadline);
        }
        self.attempts += 1;
        let capture = run_command_capture_bounded(
            &plan.repo_root,
            plan.program.as_os_str(),
            &plan.args,
            None,
            BoundedStreamLimit {
                deadline: self.deadline,
                stdout_bytes: self.stdout_remaining,
                stderr_bytes: self.stderr_remaining,
            },
        )?;
        let stdout_bytes = capture.stdout.len() as u64;
        let stderr_bytes = capture.stderr.len() as u64;
        self.stdout_remaining = self.stdout_remaining.saturating_sub(stdout_bytes);
        self.stderr_remaining = self.stderr_remaining.saturating_sub(stderr_bytes);
        self.stdout_retained += stdout_bytes;
        self.stderr_retained += stderr_bytes;
        Ok(capture)
    }
}

/// Outcome of one bounded stream reader.
#[derive(Debug)]
enum StreamOutcome {
    Complete(Vec<u8>),
    Overflow { retained_bytes: u64 },
    ReadFailed,
}

/// A minimal bounded reader used by the monitor loop and the focused
/// read-failure proof. It keeps at most `limit` bytes, reports overflow once,
/// keeps draining so a flooded writer is not blocked forever, and never
/// panics.
fn drive_bounded_reader<R: Read>(
    reader: &mut R,
    limit: u64,
    overflow: &AtomicBool,
    read_failure: &AtomicBool,
    mut report: impl FnMut(StreamOutcome),
) {
    let mut kept: Vec<u8> = Vec::new();
    let mut exceeded = false;
    let mut buffer = [0u8; BOUNDED_READ_CHUNK];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                if exceeded {
                    continue;
                }
                if kept.len() as u64 + read as u64 > limit {
                    exceeded = true;
                    overflow.store(true, Ordering::SeqCst);
                    // Report immediately so a flood is refused without waiting
                    // for EOF, then keep draining so the writer never blocks.
                    report(StreamOutcome::Overflow {
                        retained_bytes: kept.len() as u64,
                    });
                    continue;
                }
                kept.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                read_failure.store(true, Ordering::SeqCst);
                report(StreamOutcome::ReadFailed);
                return;
            }
        }
    }
    if !exceeded {
        report(StreamOutcome::Complete(kept));
    }
}

fn spawn_bounded_reader<R: Read + Send + 'static>(
    mut reader: R,
    limit: u64,
    stream: BoundedStream,
    overflow: Arc<AtomicBool>,
    read_failure: Arc<AtomicBool>,
    sender: mpsc::Sender<(BoundedStream, StreamOutcome)>,
) {
    thread::spawn(move || {
        drive_bounded_reader(&mut reader, limit, &overflow, &read_failure, |outcome| {
            let _ = sender.send((stream, outcome));
        });
    });
}

/// Run one finite command under a caller deadline and separate stream budgets.
pub(in crate::runner) fn run_command_capture_bounded(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
    limit: BoundedStreamLimit,
) -> Result<BoundedCapture, BoundedCaptureFailure> {
    let resolved_program = resolve_host_program(program);
    let label = format!(
        "{} {}",
        resolved_program.to_string_lossy(),
        render_args(args)
    );
    if Instant::now() >= limit.deadline {
        return Err(BoundedCaptureFailure::Deadline);
    }

    let mut command = ProcessCommand::new(&resolved_program);
    command.current_dir(repo_root).args(args);
    match stdin_file {
        Some(path) => {
            let file =
                std::fs::File::open(path).map_err(|error| BoundedCaptureFailure::Launch {
                    label: label.clone(),
                    error: error.to_string(),
                })?;
            command.stdin(Stdio::from(file));
        }
        None => {
            command.stdin(Stdio::null());
        }
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    // SAFETY: `pre_exec` runs after fork and before `exec`, where only
    // async-signal-safe work is allowed. `setpgid` is async-signal-safe and
    // places the child in a new process group whose id is the child's own pid,
    // which is the group `terminate_owned_group` later signals. The error path
    // uses the allocation-free `io::Error::from` conversion.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setpgid(nix::unistd::Pid::from_raw(0), nix::unistd::Pid::from_raw(0))
                .map_err(std::io::Error::from)
        });
    }
    let mut child = command
        .spawn()
        .map_err(|error| BoundedCaptureFailure::Launch {
            label: label.clone(),
            error: error.to_string(),
        })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| BoundedCaptureFailure::Launch {
            label: label.clone(),
            error: "stdout pipe unavailable".to_owned(),
        })?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| BoundedCaptureFailure::Launch {
            label: label.clone(),
            error: "stderr pipe unavailable".to_owned(),
        })?;

    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stdout_failure = Arc::new(AtomicBool::new(false));
    let stderr_failure = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel();
    spawn_bounded_reader(
        stdout,
        limit.stdout_bytes,
        BoundedStream::Stdout,
        stdout_overflow.clone(),
        stdout_failure.clone(),
        sender.clone(),
    );
    spawn_bounded_reader(
        stderr,
        limit.stderr_bytes,
        BoundedStream::Stderr,
        stderr_overflow.clone(),
        stderr_failure.clone(),
        sender.clone(),
    );
    drop(sender);

    let mut exit_status: Option<ExitStatus> = None;
    let mut stdout_outcome: Option<StreamOutcome> = None;
    let mut stderr_outcome: Option<StreamOutcome> = None;
    let mut failure: Option<BoundedCaptureFailure> = None;
    let mut channel_open = true;

    loop {
        if channel_open {
            loop {
                match receiver.try_recv() {
                    Ok((BoundedStream::Stdout, outcome)) => stdout_outcome = Some(outcome),
                    Ok((BoundedStream::Stderr, outcome)) => stderr_outcome = Some(outcome),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        channel_open = false;
                        break;
                    }
                }
            }
        }

        if exit_status.is_none() {
            exit_status = child
                .try_wait()
                .map_err(|error| BoundedCaptureFailure::Launch {
                    label: label.clone(),
                    error: error.to_string(),
                })?;
        }

        // A harvested overflow or read failure is final regardless of whether
        // the client has exited, and its retained count is exact.
        if failure.is_none() {
            failure = outcome_failure(stdout_outcome.as_ref(), stderr_outcome.as_ref(), limit);
        }
        if failure.is_none() {
            if let (
                Some(status),
                Some(StreamOutcome::Complete(stdout)),
                Some(StreamOutcome::Complete(stderr)),
            ) = (
                exit_status,
                stdout_outcome.as_ref(),
                stderr_outcome.as_ref(),
            ) {
                return Ok(BoundedCapture {
                    status,
                    stdout: stdout.clone(),
                    stderr: stderr.clone(),
                });
            }
        }
        if failure.is_none() && !channel_open {
            // Both readers ended; whichever stream never reported a complete
            // outcome is incomplete.
            failure = Some(BoundedCaptureFailure::IncompleteOutput {
                stream: if stdout_outcome.is_none() {
                    BoundedStream::Stdout
                } else {
                    BoundedStream::Stderr
                },
            });
        }
        if let Some(failure) = failure {
            terminate_owned_group(&mut child);
            return Err(failure);
        }

        if Instant::now() >= limit.deadline {
            terminate_owned_group(&mut child);
            return Err(BoundedCaptureFailure::Deadline);
        }
        thread::sleep(BOUNDED_POLL_INTERVAL);
    }
}

fn outcome_failure(
    stdout: Option<&StreamOutcome>,
    stderr: Option<&StreamOutcome>,
    limit: BoundedStreamLimit,
) -> Option<BoundedCaptureFailure> {
    // Deterministic precedence: read failure, then overflow, on stdout before
    // stderr. The choice is documented so callers can rely on it.
    fn one(
        stream: BoundedStream,
        outcome: Option<&StreamOutcome>,
        limit: BoundedStreamLimit,
    ) -> Option<BoundedCaptureFailure> {
        match outcome? {
            StreamOutcome::ReadFailed => Some(BoundedCaptureFailure::ReadFailure { stream }),
            StreamOutcome::Overflow { retained_bytes } => Some(BoundedCaptureFailure::Overflow {
                stream,
                limit_bytes: if stream == BoundedStream::Stdout {
                    limit.stdout_bytes
                } else {
                    limit.stderr_bytes
                },
                retained_bytes: *retained_bytes,
            }),
            StreamOutcome::Complete(_) => None,
        }
    }
    one(BoundedStream::Stdout, stdout, limit).or_else(|| one(BoundedStream::Stderr, stderr, limit))
}

/// SIGTERM then SIGKILL the exact owned group with a fixed bounded grace, and
/// reap the direct child. Runs whether or not the direct child already exited,
/// so an inherited pipe writer inside the group cannot outlive the command.
fn terminate_owned_group(child: &mut Child) {
    effigy_process::terminate_process_tree(child.id(), false);
    let grace = Instant::now() + BOUNDED_TERM_GRACE;
    while Instant::now() < grace {
        let _ = child.try_wait();
        thread::sleep(BOUNDED_POLL_INTERVAL);
    }
    effigy_process::terminate_process_tree(child.id(), true);
    let _ = child.kill();
    let _ = child.wait();
}

fn render_args(args: &[OsString]) -> String {
    args.iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    fn stream_limit(deadline: Instant, stdout_bytes: u64, stderr_bytes: u64) -> BoundedStreamLimit {
        BoundedStreamLimit {
            deadline,
            stdout_bytes,
            stderr_bytes,
        }
    }

    #[cfg(unix)]
    fn test_command(script: &str) -> (tempfile::TempDir, OsString, Vec<OsString>) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("fake-backend.sh");
        let mut file = std::fs::File::create(&path).expect("create fake backend");
        file.write_all(script.as_bytes())
            .expect("write fake backend");
        drop(file);
        let mut perms = std::fs::metadata(&path).expect("metadata").permissions();
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod");
        (dir, OsString::from("/bin/sh"), vec![path.into_os_string()])
    }

    #[test]
    fn bounded_reader_keeps_exactly_the_budget() {
        let overflow = AtomicBool::new(false);
        let failure = AtomicBool::new(false);
        let mut input = std::io::Cursor::new(vec![b'a'; 32]);
        let mut outcome = None;
        drive_bounded_reader(&mut input, 32, &overflow, &failure, |reported| {
            outcome = Some(reported);
        });
        match outcome.expect("reader reports an outcome") {
            StreamOutcome::Complete(bytes) => assert_eq!(bytes.len(), 32),
            other => panic!("expected complete, got {other:?}"),
        }
        assert!(!overflow.load(Ordering::SeqCst));
    }

    #[test]
    fn bounded_reader_reports_one_byte_over_and_retains_the_prefix() {
        let overflow = AtomicBool::new(false);
        let failure = AtomicBool::new(false);
        // Deliberately short reads so the retained prefix reaches the budget
        // before the overflowing byte arrives.
        struct Chunked(Vec<u8>, usize);
        impl Read for Chunked {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                let take = self.1.min(buffer.len()).min(self.0.len());
                buffer[..take].copy_from_slice(&self.0[..take]);
                self.0.drain(..take);
                Ok(take)
            }
        }
        let mut input = Chunked(vec![b'a'; 33], 8);
        let mut outcome = None;
        drive_bounded_reader(&mut input, 32, &overflow, &failure, |reported| {
            outcome = Some(reported);
        });
        match outcome.expect("reader reports an outcome") {
            StreamOutcome::Overflow { retained_bytes } => {
                assert!(retained_bytes <= 32, "retention must stay bounded");
            }
            other => panic!("expected overflow, got {other:?}"),
        }
        assert!(overflow.load(Ordering::SeqCst));
    }

    #[test]
    fn bounded_reader_reports_read_failure_without_panicking() {
        struct FailingReader;
        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("private read fault"))
            }
        }
        let overflow = AtomicBool::new(false);
        let failure = AtomicBool::new(false);
        let mut reader = FailingReader;
        let mut outcome = None;
        drive_bounded_reader(&mut reader, 64, &overflow, &failure, |reported| {
            outcome = Some(reported);
        });
        assert!(matches!(outcome, Some(StreamOutcome::ReadFailed)));
        assert!(failure.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn exact_boundary_is_complete_success_and_one_byte_over_refuses() {
        let (_dir, program, args) = test_command("printf 'aaaa'");
        let deadline = Instant::now() + Duration::from_secs(30);
        let capture = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(deadline, 4, 4),
        )
        .expect("exact boundary must succeed");
        assert!(capture.status.success());
        assert_eq!(capture.stdout, b"aaaa");

        let (_dir, program, args) = test_command("printf 'aaaaa'");
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_secs(30), 4, 4),
        )
        .expect_err("one byte over must refuse");
        assert!(matches!(
            error,
            BoundedCaptureFailure::Overflow {
                stream: BoundedStream::Stdout,
                ..
            }
        ));
    }

    #[cfg(unix)]
    #[test]
    fn simultaneous_flood_refuses_without_selecting_a_partial_prefix() {
        // Both streams exceed their budgets; neither may be reported as success.
        let (_dir, program, args) = test_command(
            "i=0; while [ $i -lt 4096 ]; do printf 'o'; printf 'e' 1>&2; i=$((i+1)); done",
        );
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_secs(30), 8, 8),
        )
        .expect_err("flood must refuse");
        assert!(matches!(error, BoundedCaptureFailure::Overflow { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn asymmetric_flood_of_one_stream_still_terminates() {
        // stderr floods while stdout stays empty; the operation must not hang
        // waiting on the quiet stream.
        let (_dir, program, args) =
            test_command("i=0; while [ $i -lt 100000 ]; do printf 'e' 1>&2; i=$((i+1)); done");
        let started = Instant::now();
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_secs(30), 4, 16),
        )
        .expect_err("asymmetric flood must refuse");
        assert!(matches!(
            error,
            BoundedCaptureFailure::Overflow {
                stream: BoundedStream::Stderr,
                ..
            }
        ));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(unix)]
    #[test]
    fn expired_deadline_refuses_before_spawn() {
        let error = run_command_capture_bounded(
            Path::new("."),
            OsStr::new("/definitely-not-a-binary-effigy-bounded-probe"),
            &[],
            None,
            stream_limit(
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("expired deadline"),
                4,
                4,
            ),
        )
        .expect_err("expired deadline must refuse");
        assert!(matches!(error, BoundedCaptureFailure::Deadline));
    }

    #[cfg(unix)]
    #[test]
    fn deadline_reaps_owned_group_and_leaves_a_foreign_process_alone() {
        // The owned child sleeps longer than the deadline; the foreign
        // sentinel is a separate process that must survive.
        let (_dir, program, args) = test_command("exec /bin/sleep 30");
        let mut foreign = ProcessCommand::new("/bin/sleep")
            .arg("30")
            .spawn()
            .expect("foreign child");
        let started = Instant::now();
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_millis(300), 64, 64),
        )
        .expect_err("slow command must hit the deadline");
        assert!(matches!(error, BoundedCaptureFailure::Deadline));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "deadline must terminate and reap instead of waiting out the child"
        );
        assert!(
            foreign.try_wait().expect("foreign wait").is_none(),
            "a foreign process must not be signaled"
        );
        let _ = foreign.kill();
        let _ = foreign.wait();
    }

    #[cfg(unix)]
    #[test]
    fn inherited_pipe_writer_cannot_outlive_the_deadline() {
        // The direct client exits immediately but a background descendant in
        // the same owned group keeps the pipe open. Without the deadline the
        // final drain would block forever.
        let (_dir, program, args) = test_command("(sleep 30) & exit 0");
        let started = Instant::now();
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_millis(400), 64, 64),
        )
        .expect_err("an inherited writer must not defeat the deadline");
        assert!(matches!(error, BoundedCaptureFailure::Deadline));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn complete_capture_reports_the_inner_exit_status() {
        let (_dir, program, args) = test_command("printf 'out'; printf 'err' 1>&2; exit 3");
        let capture = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            stream_limit(Instant::now() + Duration::from_secs(30), 64, 64),
        )
        .expect("capture within budget");
        assert_eq!(capture.status.code(), Some(3));
        assert_eq!(capture.stdout, b"out");
        assert_eq!(capture.stderr, b"err");
    }

    #[cfg(unix)]
    #[test]
    fn cumulative_budget_is_not_reset_by_a_second_attempt() {
        // First attempt retains 3 bytes of stdout; the second attempt may only
        // retain one more before the total budget is exhausted.
        let (_dir, program, args) = test_command("printf 'abc'");
        let mut budget = BoundedCommandBudget::new(Instant::now() + Duration::from_secs(30), 4, 4);
        let plan = plan_for(Path::new("."), &program, &args);
        budget.run_plan(&plan).expect("first attempt");
        assert_eq!(budget.stdout_retained(), 3);
        let (_dir2, program2, args2) = test_command("printf 'de'");
        let plan2 = plan_for(Path::new("."), &program2, &args2);
        let error = budget
            .run_plan(&plan2)
            .expect_err("second attempt must respect the remaining budget");
        assert!(matches!(
            error,
            BoundedCaptureFailure::Overflow {
                stream: BoundedStream::Stdout,
                limit_bytes: 1,
                ..
            }
        ));
        assert_eq!(budget.attempts(), 2);
    }

    fn plan_for(
        repo_root: &Path,
        program: &OsStr,
        args: &[OsString],
    ) -> effigy_containers::ContainerComposeInvocationPlan {
        effigy_containers::ContainerComposeInvocationPlan {
            backend_id: effigy_containers::BackendId::docker_compose(),
            repo_root: repo_root.to_path_buf(),
            profile: "effigy".to_owned(),
            action: effigy_containers::ContainerAction::Exec,
            program: program.to_os_string(),
            args: args.to_vec(),
            label: "private bounded fixture".to_owned(),
        }
    }
}
