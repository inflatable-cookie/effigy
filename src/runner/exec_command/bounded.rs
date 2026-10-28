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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

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
/// Version of the structural refusal observation record attached to bounded
/// transport refusals. Bump when a field changes meaning; purely additive
/// fields keep this version.
pub(in crate::runner) const BOUNDED_REFUSAL_OBSERVATIONS_VERSION: u32 = 1;

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
    Launch,
}

/// Operation phase for a subprocess captured by one bounded container shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedCommandPhase {
    RuntimeDiscovery,
    WorkspacePreparation,
    CommandExecution,
}

impl BoundedCommandPhase {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::RuntimeDiscovery => "runtime_discovery",
            Self::WorkspacePreparation => "workspace_preparation",
            Self::CommandExecution => "command_execution",
        }
    }
}

/// A bounded capture failure tagged with the operation phase that launched it,
/// plus the versioned structural observations taken at one coherent snapshot
/// when the refusal was decided. Observations are boxed: refusals are rare,
/// and callers pass the failure through result types.
#[derive(Debug)]
pub(in crate::runner) struct BoundedScopedCaptureFailure {
    pub(in crate::runner) phase: BoundedCommandPhase,
    pub(in crate::runner) failure: BoundedCaptureFailure,
    pub(in crate::runner) observations: Box<BoundedRefusalObservations>,
}

/// A bounded transport refusal plus the observations from its snapshot.
#[derive(Debug)]
pub(in crate::runner) struct BoundedTransportRefusal {
    pub(in crate::runner) failure: BoundedCaptureFailure,
    pub(in crate::runner) observations: Box<BoundedRefusalObservations>,
}

/// How far the refused capture's host spawn transition observably reached,
/// from real transition observations only. This describes the owned host
/// client process; it is never a claim that a remote or guest process began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedSpawnDisposition {
    /// The refusal was decided before the host spawn syscall was issued.
    NotAttempted,
    /// A host spawn was attempted and failed; no child could be confirmed.
    AttemptedNotConfirmed,
    /// A host client child process was created for this capture.
    Spawned,
    /// The spawn transition could not be observed. Current transitions always
    /// know this truth, but `unknown` is part of the versioned observation
    /// vocabulary and stays available for readers of the refusal payload.
    #[allow(dead_code)]
    Unknown,
}

impl BoundedSpawnDisposition {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::NotAttempted => "not_attempted",
            Self::AttemptedNotConfirmed => "attempted_not_confirmed",
            Self::Spawned => "spawned",
            Self::Unknown => "unknown",
        }
    }
}

/// One stream's observed reading state at a refusal snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedStreamRefusalState {
    /// The reader was still reading at the snapshot; its counts are lower
    /// bounds and end-of-stream is not established.
    Pending,
    /// The reader reached end-of-stream; its retained count is exact.
    Eof,
    /// The reader exceeded this attempt's retention budget. The retained
    /// prefix is exact; reads continued only to keep the writer unblocked.
    Overflow,
    /// The reader hit a read error and stopped.
    ReadError,
    /// No stream observation exists: no reader ran, or a reader ended
    /// without reporting an outcome.
    Unknown,
}

impl BoundedStreamRefusalState {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Eof => "eof",
            Self::Overflow => "overflow",
            Self::ReadError => "read_error",
            Self::Unknown => "unknown",
        }
    }
}

/// A byte count with explicit observation semantics. A missing observation
/// is [`BoundedObservedBytes::Unknown`], never zero and never proof that no
/// bytes flowed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedObservedBytes {
    /// The count is exact at the snapshot.
    Exact(u64),
    /// The true total is at least this count.
    LowerBound(u64),
    /// No truthful count is available.
    Unknown,
}

/// Pretermination leader observation, taken only from the existing safe
/// in-budget `try_wait` window. A non-reaping exit observation is not
/// supported by the bounded path, so an unobserved leader stays unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum BoundedLeaderExitObservation {
    /// The leader was still running at the last safe observation (the
    /// existing `try_wait` only runs once both streams are complete, and
    /// returning `None` reaps nothing).
    Alive,
    /// No safe pretermination exit observation exists. This is explicit
    /// unknown: it is not a claim that the leader exited or survived.
    Unknown,
}

impl BoundedLeaderExitObservation {
    pub(in crate::runner) fn label(self) -> &'static str {
        match self {
            Self::Alive => "alive",
            Self::Unknown => "unknown",
        }
    }
}

/// One stream's structural observation at one coherent refusal snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct BoundedStreamRefusalObservation {
    pub(in crate::runner) state: BoundedStreamRefusalState,
    /// The bounded retained prefix at the snapshot. For a refused attempt
    /// these bytes are the failed prefix: they are reported here and never
    /// added to completed-capture retained totals or charged to the
    /// cumulative caps a second time.
    pub(in crate::runner) retained_prefix: BoundedObservedBytes,
    /// Total bytes read off the stream: retained bytes plus discarded
    /// overflow, so a flood is visible even when retention stopped early.
    pub(in crate::runner) read_total: BoundedObservedBytes,
}

/// Versioned, secret-free structural observations from one bounded refusal
/// snapshot. Everything here comes from the original monotonic operation
/// clock and the capture's own transitions: no bytes, argv, environment,
/// raw errors, content digests or names. Missing observations are explicitly
/// unknown, never zero and never proof of no effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) struct BoundedRefusalObservations {
    pub(in crate::runner) version: u32,
    /// Monotonic elapsed from the original operation origin to the snapshot.
    pub(in crate::runner) operation_elapsed: Duration,
    /// Remaining budget at the snapshot; zero once the deadline has passed.
    pub(in crate::runner) remaining_budget: Duration,
    /// Elapsed from the operation origin to the refused attempt's start.
    /// Absent when no attempt began.
    pub(in crate::runner) preparation_elapsed: Option<Duration>,
    /// Elapsed from the refused attempt's start to the snapshot. Absent when
    /// no attempt began.
    pub(in crate::runner) attempt_elapsed: Option<Duration>,
    pub(in crate::runner) spawn_disposition: BoundedSpawnDisposition,
    pub(in crate::runner) stdout: BoundedStreamRefusalObservation,
    pub(in crate::runner) stderr: BoundedStreamRefusalObservation,
    pub(in crate::runner) leader_exit: BoundedLeaderExitObservation,
}

impl BoundedRefusalObservations {
    /// Observations for a refusal decided at a budget boundary before any
    /// transport attempt of this step: no spawn, stream or leader of its own
    /// exists to observe, so those say so explicitly.
    pub(in crate::runner) fn budget_boundary(
        origin: Instant,
        deadline: Instant,
        now: Instant,
    ) -> Self {
        let stream = BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::Unknown,
            retained_prefix: BoundedObservedBytes::Unknown,
            read_total: BoundedObservedBytes::Unknown,
        };
        Self {
            version: BOUNDED_REFUSAL_OBSERVATIONS_VERSION,
            operation_elapsed: now.saturating_duration_since(origin),
            remaining_budget: deadline.saturating_duration_since(now),
            preparation_elapsed: None,
            attempt_elapsed: None,
            spawn_disposition: BoundedSpawnDisposition::NotAttempted,
            stdout: stream,
            stderr: stream,
            leader_exit: BoundedLeaderExitObservation::Unknown,
        }
    }
}

/// Byte progress shared from each reader thread to the monitor loop so a
/// pending stream can report a truthful lower bound at a refusal snapshot.
#[derive(Debug, Default)]
pub(in crate::runner) struct BoundedStreamProgress {
    pub(in crate::runner) retained: AtomicU64,
    pub(in crate::runner) read: AtomicU64,
}

/// One bounded operation's original monotonic origin, absolute deadline and
/// per-stream remaining budgets.
#[derive(Debug, Clone, Copy)]
pub(in crate::runner) struct BoundedStreamLimit {
    /// The original operation clock origin this capture belongs to. All
    /// refusal clock observations are measured from it; it is never derived
    /// from an external deadline or rebased.
    pub(in crate::runner) origin: Instant,
    pub(in crate::runner) deadline: Instant,
    pub(in crate::runner) stdout_bytes: u64,
    pub(in crate::runner) stderr_bytes: u64,
}

// Opt-in, thread-local recording for the full public-entry proof. This records
// the actual limit passed to the production reader, without replacing clocks,
// warming phases or changing release-build behavior.
#[cfg(test)]
std::thread_local! {
    static CAPTURE_LIMIT_TRACE: std::cell::RefCell<Option<Vec<(BoundedCommandPhase, BoundedStreamLimit)>>> = const { std::cell::RefCell::new(None) };
}

/// Cumulative retention budget for one bounded container operation.
///
/// A single absolute deadline and a single pair of stream budgets live here.
/// Every captured command in the operation — including any eligible existing
/// identity fallback — draws from the same remaining budget, so the hard total
/// cannot be reset by a second attempt.
#[derive(Debug)]
pub(in crate::runner) struct BoundedCommandBudget {
    /// The original monotonic operation origin, created before discovery and
    /// preparation and never rebased. Refusal clock observations are
    /// measured from it.
    origin: Instant,
    deadline: Instant,
    stdout_remaining: u64,
    stderr_remaining: u64,
    stdout_retained: u64,
    stderr_retained: u64,
    attempts: u32,
    preparation_captures: u32,
    preparation_stdout_retained: u64,
    preparation_stderr_retained: u64,
}

impl BoundedCommandBudget {
    #[cfg(test)]
    pub(in crate::runner) fn with_capture_limit_trace<T>(
        run: impl FnOnce() -> T,
    ) -> (T, Vec<(BoundedCommandPhase, BoundedStreamLimit)>) {
        struct RestoreTrace;
        impl Drop for RestoreTrace {
            fn drop(&mut self) {
                CAPTURE_LIMIT_TRACE.with(|trace| *trace.borrow_mut() = None);
            }
        }
        CAPTURE_LIMIT_TRACE.with(|trace| {
            assert!(trace.borrow().is_none(), "nested capture proof");
            *trace.borrow_mut() = Some(Vec::new());
        });
        let _restore = RestoreTrace;
        let result = run();
        let trace = CAPTURE_LIMIT_TRACE.with(|trace| trace.borrow_mut().take().unwrap());
        (result, trace)
    }

    pub(in crate::runner) fn new(
        origin: Instant,
        deadline: Instant,
        stdout_bytes: u64,
        stderr_bytes: u64,
    ) -> Self {
        Self {
            origin,
            deadline,
            stdout_remaining: stdout_bytes,
            stderr_remaining: stderr_bytes,
            stdout_retained: 0,
            stderr_retained: 0,
            attempts: 0,
            preparation_captures: 0,
            preparation_stdout_retained: 0,
            preparation_stderr_retained: 0,
        }
    }

    /// Truthful clock observations for a refusal decided at a budget
    /// boundary, from the original operation origin, with no spawn, stream
    /// or leader of the refused step to observe.
    pub(in crate::runner) fn boundary_observations(&self) -> BoundedRefusalObservations {
        BoundedRefusalObservations::budget_boundary(self.origin, self.deadline, Instant::now())
    }

    /// A refusal decided at a budget boundary before this step's transport
    /// attempt, carrying those truthful boundary observations.
    pub(in crate::runner) fn boundary_refusal(
        &self,
        phase: BoundedCommandPhase,
        failure: BoundedCaptureFailure,
    ) -> BoundedScopedCaptureFailure {
        BoundedScopedCaptureFailure {
            phase,
            failure,
            observations: Box::new(self.boundary_observations()),
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

    pub(in crate::runner) fn preparation_captures(&self) -> u32 {
        self.preparation_captures
    }

    pub(in crate::runner) fn preparation_stdout_retained(&self) -> u64 {
        self.preparation_stdout_retained
    }

    pub(in crate::runner) fn preparation_stderr_retained(&self) -> u64 {
        self.preparation_stderr_retained
    }

    /// Remaining budget for the workspace artifact rehearsal probes, which are
    /// their own cfg(test) deadlines. No shipping caller needs it: the bounded
    /// workspace entry path verifies the pinned guest instead of acquiring one.
    #[cfg(test)]
    pub(in crate::runner) fn remaining_time(&self) -> Option<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        (!remaining.is_zero()).then_some(remaining)
    }

    /// Run one resolved command plan within the remaining budget and account
    /// its retained bytes cumulatively for the selected phase.
    ///
    /// Callers must have proved which container the plan targets: the bounded
    /// workspace preparation path converts its Compose plan to runtime
    /// arguments for the verified container ID before reaching this call.
    pub(in crate::runner) fn run_command(
        &mut self,
        repo_root: &Path,
        program: &OsStr,
        args: &[OsString],
        stdin_file: Option<&Path>,
        env: &[(OsString, OsString)],
        phase: BoundedCommandPhase,
    ) -> Result<BoundedCapture, BoundedScopedCaptureFailure> {
        // An expired deadline refuses before any spawn, so it is not counted as
        // an attempt. The boundary observations record that truthfully: not
        // attempted, with no stream or leader of this step to observe.
        if Instant::now() >= self.deadline {
            return Err(self.boundary_refusal(phase, BoundedCaptureFailure::Deadline));
        }
        match phase {
            BoundedCommandPhase::CommandExecution => self.attempts += 1,
            BoundedCommandPhase::RuntimeDiscovery | BoundedCommandPhase::WorkspacePreparation => {
                self.preparation_captures += 1
            }
        }
        // The attempt boundary on the original operation clock: preparation
        // observations run from the origin to here, attempt observations from
        // here to the refusal snapshot. This is a strictly internal join on
        // the same clock, not a rebased or invented origin.
        let attempt_started = Instant::now();
        let limit = BoundedStreamLimit {
            origin: self.origin,
            deadline: self.deadline,
            stdout_bytes: self.stdout_remaining,
            stderr_bytes: self.stderr_remaining,
        };
        #[cfg(test)]
        CAPTURE_LIMIT_TRACE.with(|trace| {
            if let Some(trace) = trace.borrow_mut().as_mut() {
                trace.push((phase, limit));
            }
        });
        let capture = run_command_capture_bounded(
            repo_root,
            program,
            args,
            stdin_file,
            env,
            limit,
            attempt_started,
        )
        .map_err(|refusal| BoundedScopedCaptureFailure {
            phase,
            failure: refusal.failure,
            observations: refusal.observations,
        })?;
        let stdout_bytes = capture.stdout.len() as u64;
        let stderr_bytes = capture.stderr.len() as u64;
        self.stdout_remaining = self.stdout_remaining.saturating_sub(stdout_bytes);
        self.stderr_remaining = self.stderr_remaining.saturating_sub(stderr_bytes);
        self.stdout_retained += stdout_bytes;
        self.stderr_retained += stderr_bytes;
        if phase != BoundedCommandPhase::CommandExecution {
            self.preparation_stdout_retained += stdout_bytes;
            self.preparation_stderr_retained += stderr_bytes;
        }
        Ok(capture)
    }
}

/// Outcome of one bounded stream reader.
#[derive(Debug)]
pub(in crate::runner) enum StreamOutcome {
    Complete(Vec<u8>),
    Overflow { retained_bytes: u64 },
    ReadFailed,
}

/// A minimal bounded reader used by the monitor loop and the focused
/// read-failure proof. It keeps at most `limit` bytes, reports overflow once,
/// keeps draining so a flooded writer is not blocked forever, and never
/// panics. Every kept or discarded byte is added to `progress` so a refusal
/// snapshot can report a truthful lower bound for a still-pending stream.
fn drive_bounded_reader<R: Read>(
    reader: &mut R,
    limit: u64,
    overflow: &AtomicBool,
    read_failure: &AtomicBool,
    progress: &BoundedStreamProgress,
    mut report: impl FnMut(StreamOutcome),
) {
    let mut kept: Vec<u8> = Vec::new();
    let mut exceeded = false;
    let mut buffer = [0u8; BOUNDED_READ_CHUNK];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                progress.read.fetch_add(read as u64, Ordering::SeqCst);
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
                progress.retained.fetch_add(read as u64, Ordering::SeqCst);
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

/// Build one stream's refusal-snapshot observation from its harvested
/// outcome (if any), its shared progress counters and whether its reader
/// already ended without reporting an outcome. The channel delivery of an
/// outcome orders the reader's final counter writes before this load, so a
/// stopped reader's counts are exact; a pending reader's are lower bounds.
pub(in crate::runner) fn refusal_stream_observation(
    outcome: Option<&StreamOutcome>,
    progress: &BoundedStreamProgress,
    reader_ended_without_outcome: bool,
) -> BoundedStreamRefusalObservation {
    let retained_now = progress.retained.load(Ordering::SeqCst);
    let read_now = progress.read.load(Ordering::SeqCst);
    match outcome {
        Some(StreamOutcome::Complete(bytes)) => BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::Eof,
            retained_prefix: BoundedObservedBytes::Exact(bytes.len() as u64),
            read_total: BoundedObservedBytes::Exact(read_now),
        },
        Some(StreamOutcome::Overflow { retained_bytes }) => BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::Overflow,
            retained_prefix: BoundedObservedBytes::Exact(*retained_bytes),
            // An overflowing stream keeps draining discarded bytes until
            // EOF, so the read total can still grow after the snapshot.
            read_total: BoundedObservedBytes::LowerBound(read_now),
        },
        Some(StreamOutcome::ReadFailed) => BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::ReadError,
            // The reader stopped at the error, so both counters are final.
            retained_prefix: BoundedObservedBytes::Exact(retained_now),
            read_total: BoundedObservedBytes::Exact(read_now),
        },
        None if reader_ended_without_outcome => BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::Unknown,
            // The reader kept bytes it never reported; the counter is a
            // truthful lower bound on that lost prefix, not a zero claim.
            retained_prefix: BoundedObservedBytes::LowerBound(retained_now),
            read_total: BoundedObservedBytes::Exact(read_now),
        },
        None => BoundedStreamRefusalObservation {
            state: BoundedStreamRefusalState::Pending,
            retained_prefix: BoundedObservedBytes::LowerBound(retained_now),
            read_total: BoundedObservedBytes::LowerBound(read_now),
        },
    }
}

/// Assemble the versioned refusal observations from one coherent snapshot
/// instant: all clock facts derive from that single `now`, the original
/// operation origin and the refused attempt's start.
fn refusal_observations(
    limit: BoundedStreamLimit,
    attempt_started: Instant,
    now: Instant,
    spawn_disposition: BoundedSpawnDisposition,
    stdout: BoundedStreamRefusalObservation,
    stderr: BoundedStreamRefusalObservation,
    leader_exit: BoundedLeaderExitObservation,
) -> BoundedRefusalObservations {
    BoundedRefusalObservations {
        version: BOUNDED_REFUSAL_OBSERVATIONS_VERSION,
        operation_elapsed: now.saturating_duration_since(limit.origin),
        remaining_budget: limit.deadline.saturating_duration_since(now),
        preparation_elapsed: Some(attempt_started.saturating_duration_since(limit.origin)),
        attempt_elapsed: Some(now.saturating_duration_since(attempt_started)),
        spawn_disposition,
        stdout,
        stderr,
        leader_exit,
    }
}

/// A snapshot with no stream observation at all (no reader ever ran).
fn unobserved_stream() -> BoundedStreamRefusalObservation {
    BoundedStreamRefusalObservation {
        state: BoundedStreamRefusalState::Unknown,
        retained_prefix: BoundedObservedBytes::Unknown,
        read_total: BoundedObservedBytes::Unknown,
    }
}

fn spawn_bounded_reader<R: Read + Send + 'static>(
    mut reader: R,
    limit: u64,
    stream: BoundedStream,
    overflow: Arc<AtomicBool>,
    read_failure: Arc<AtomicBool>,
    progress: Arc<BoundedStreamProgress>,
    sender: mpsc::Sender<(BoundedStream, StreamOutcome)>,
) {
    thread::spawn(move || {
        drive_bounded_reader(
            &mut reader,
            limit,
            &overflow,
            &read_failure,
            &progress,
            |outcome| {
                let _ = sender.send((stream, outcome));
            },
        );
    });
}

/// Run one finite command under a caller deadline and separate stream budgets.
/// A refusal carries versioned structural observations from one coherent
/// snapshot at its decision point; the snapshot never calls `try_wait`, so it
/// cannot reap the leader or observe anything the existing policy forbids.
pub(in crate::runner) fn run_command_capture_bounded(
    repo_root: &Path,
    program: &OsStr,
    args: &[OsString],
    stdin_file: Option<&Path>,
    env: &[(OsString, OsString)],
    limit: BoundedStreamLimit,
    attempt_started: Instant,
) -> Result<BoundedCapture, BoundedTransportRefusal> {
    let resolved_program = resolve_host_program(program);
    if Instant::now() >= limit.deadline {
        let now = Instant::now();
        return Err(BoundedTransportRefusal {
            failure: BoundedCaptureFailure::Deadline,
            observations: Box::new(refusal_observations(
                limit,
                attempt_started,
                now,
                BoundedSpawnDisposition::NotAttempted,
                unobserved_stream(),
                unobserved_stream(),
                BoundedLeaderExitObservation::Unknown,
            )),
        });
    }

    let mut command = ProcessCommand::new(&resolved_program);
    command.current_dir(repo_root).args(args);
    for (key, value) in env {
        command.env(key, value);
    }
    let launch_refusal = |disposition, now: Instant| BoundedTransportRefusal {
        failure: BoundedCaptureFailure::Launch,
        observations: Box::new(refusal_observations(
            limit,
            attempt_started,
            now,
            disposition,
            unobserved_stream(),
            unobserved_stream(),
            BoundedLeaderExitObservation::Unknown,
        )),
    };
    match stdin_file {
        Some(path) => {
            // The spawn syscall was never issued, so this is not an attempt.
            let file = std::fs::File::open(path).map_err(|_| {
                launch_refusal(BoundedSpawnDisposition::NotAttempted, Instant::now())
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
    let mut child = command.spawn().map_err(|_| {
        launch_refusal(
            BoundedSpawnDisposition::AttemptedNotConfirmed,
            Instant::now(),
        )
    })?;
    // A child exists from here on, whatever later refusals report.
    let spawned = BoundedSpawnDisposition::Spawned;

    let Some(stdout) = child.stdout.take() else {
        terminate_owned_group(&mut child);
        return Err(launch_refusal(spawned, Instant::now()));
    };
    let Some(stderr) = child.stderr.take() else {
        terminate_owned_group(&mut child);
        return Err(launch_refusal(spawned, Instant::now()));
    };

    let stdout_overflow = Arc::new(AtomicBool::new(false));
    let stderr_overflow = Arc::new(AtomicBool::new(false));
    let stdout_failure = Arc::new(AtomicBool::new(false));
    let stderr_failure = Arc::new(AtomicBool::new(false));
    let stdout_progress = Arc::new(BoundedStreamProgress::default());
    let stderr_progress = Arc::new(BoundedStreamProgress::default());
    let (sender, receiver) = mpsc::channel();
    spawn_bounded_reader(
        stdout,
        limit.stdout_bytes,
        BoundedStream::Stdout,
        stdout_overflow.clone(),
        stdout_failure.clone(),
        stdout_progress.clone(),
        sender.clone(),
    );
    spawn_bounded_reader(
        stderr,
        limit.stderr_bytes,
        BoundedStream::Stderr,
        stderr_overflow.clone(),
        stderr_failure.clone(),
        stderr_progress.clone(),
        sender.clone(),
    );
    drop(sender);

    let mut exit_status: Option<ExitStatus> = None;
    // True only after the existing safe `try_wait` window observed the leader
    // still running (returning None reaps nothing). No other pretermination
    // observation exists, so an unobserved leader stays explicitly unknown.
    let mut leader_observed_alive = false;
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

        // Keep the leader unreaped while an inherited pipe writer can remain.
        // Its PID/group therefore cannot be reused before owned cancellation.
        let streams_complete = matches!(stdout_outcome, Some(StreamOutcome::Complete(_)))
            && matches!(stderr_outcome, Some(StreamOutcome::Complete(_)));
        if Instant::now() >= limit.deadline {
            // Snapshot before termination: one coherent decision instant, no
            // diagnostic try_wait, no reap, no post-deadline observation.
            let now = Instant::now();
            let reader_ended = !channel_open;
            let leader_exit = if leader_observed_alive {
                BoundedLeaderExitObservation::Alive
            } else {
                BoundedLeaderExitObservation::Unknown
            };
            let observations = refusal_observations(
                limit,
                attempt_started,
                now,
                spawned,
                refusal_stream_observation(stdout_outcome.as_ref(), &stdout_progress, reader_ended),
                refusal_stream_observation(stderr_outcome.as_ref(), &stderr_progress, reader_ended),
                leader_exit,
            );
            terminate_owned_group(&mut child);
            return Err(BoundedTransportRefusal {
                failure: BoundedCaptureFailure::Deadline,
                observations: Box::new(observations),
            });
        }
        if exit_status.is_none() && streams_complete {
            match child.try_wait() {
                Ok(None) => leader_observed_alive = true,
                Ok(status @ Some(_)) => exit_status = status,
                Err(_) => {
                    terminate_owned_group(&mut child);
                    return Err(launch_refusal(spawned, Instant::now()));
                }
            }
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
        if failure.is_none() && !channel_open && !streams_complete {
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
            // Snapshot at the decision instant, before the fixed termination
            // grace: still no diagnostic try_wait and no early reap.
            let now = Instant::now();
            let reader_ended = !channel_open;
            let observations = refusal_observations(
                limit,
                attempt_started,
                now,
                spawned,
                refusal_stream_observation(stdout_outcome.as_ref(), &stdout_progress, reader_ended),
                refusal_stream_observation(stderr_outcome.as_ref(), &stderr_progress, reader_ended),
                BoundedLeaderExitObservation::Unknown,
            );
            terminate_owned_group(&mut child);
            return Err(BoundedTransportRefusal {
                failure,
                observations: Box::new(observations),
            });
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
pub(super) fn terminate_owned_group(child: &mut Child) {
    #[cfg(unix)]
    let group = {
        // Both callers create this group before exec and retain its unreaped
        // leader. No process-table command, inferred group or reusable PID is
        // needed to signal it. Never use killpg's platform-specific 0/1 forms.
        let pid = i32::try_from(child.id()).expect("Unix child PID fits pid_t");
        assert!(pid > 1, "owned capture leader cannot be PID 0 or 1");
        let group = nix::unistd::Pid::from_raw(pid);
        let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGTERM);
        group
    };
    // Do not reap the leader between TERM and KILL: even a zombie reserves
    // the PID/group while descendants may still hold inherited pipes open.
    thread::sleep(BOUNDED_TERM_GRACE);
    #[cfg(unix)]
    let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::AtomicBool;

    fn stream_limit(deadline: Instant, stdout_bytes: u64, stderr_bytes: u64) -> BoundedStreamLimit {
        // Tests may pass already-expired deadlines; the origin stays before
        // both now and the deadline so clock facts stay monotonic.
        let now = Instant::now();
        BoundedStreamLimit {
            origin: now.min(deadline),
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
        drive_bounded_reader(
            &mut input,
            32,
            &overflow,
            &failure,
            &BoundedStreamProgress::default(),
            |reported| {
                outcome = Some(reported);
            },
        );
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
        drive_bounded_reader(
            &mut input,
            32,
            &overflow,
            &failure,
            &BoundedStreamProgress::default(),
            |reported| {
                outcome = Some(reported);
            },
        );
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
        // A real owned reader that delivers bytes and then fails: this drives
        // the exact production reader loop the spawned-pipe readers run, so
        // the ReadFailed outcome, stop behavior and final counters are
        // proven on production code. (A genuine pipe read error is not
        // inducible through the spawned production path; see contract 005.)
        struct BytesThenFailure {
            delivered: bool,
        }
        impl Read for BytesThenFailure {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if self.delivered {
                    Err(std::io::Error::other("private read fault"))
                } else {
                    self.delivered = true;
                    buffer[..5].copy_from_slice(b"12345");
                    Ok(5)
                }
            }
        }
        let overflow = AtomicBool::new(false);
        let failure = AtomicBool::new(false);
        let progress = BoundedStreamProgress::default();
        let mut reader = BytesThenFailure { delivered: false };
        let mut outcome = None;
        drive_bounded_reader(
            &mut reader,
            64,
            &overflow,
            &failure,
            &progress,
            |reported| {
                outcome = Some(reported);
            },
        );
        assert!(matches!(outcome, Some(StreamOutcome::ReadFailed)));
        assert!(failure.load(Ordering::SeqCst));
        // The reader stopped at the error, so its counters are final and
        // exact: the refused snapshot may report them as such.
        assert_eq!(progress.retained.load(Ordering::SeqCst), 5);
        assert_eq!(progress.read.load(Ordering::SeqCst), 5);
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
            &[],
            stream_limit(deadline, 4, 4),
            Instant::now(),
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
            &[],
            stream_limit(Instant::now() + Duration::from_secs(30), 4, 4),
            Instant::now(),
        )
        .expect_err("one byte over must refuse");
        assert!(matches!(
            error.failure,
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
            &[],
            stream_limit(Instant::now() + Duration::from_secs(30), 8, 8),
            Instant::now(),
        )
        .expect_err("flood must refuse");
        assert!(matches!(
            error.failure,
            BoundedCaptureFailure::Overflow { .. }
        ));
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
            &[],
            stream_limit(Instant::now() + Duration::from_secs(30), 4, 16),
            Instant::now(),
        )
        .expect_err("asymmetric flood must refuse");
        assert!(matches!(
            error.failure,
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
            &[],
            stream_limit(
                Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("expired deadline"),
                4,
                4,
            ),
            Instant::now(),
        )
        .expect_err("expired deadline must refuse");
        assert!(matches!(error.failure, BoundedCaptureFailure::Deadline));
        // A true pre-spawn expiry is not a spawn: the refusal says so
        // explicitly instead of implying it from the attempt count.
        assert_eq!(
            error.observations.spawn_disposition,
            BoundedSpawnDisposition::NotAttempted
        );
        assert_eq!(
            error.observations.version,
            BOUNDED_REFUSAL_OBSERVATIONS_VERSION
        );
        assert_eq!(
            error.observations.stdout.state,
            BoundedStreamRefusalState::Unknown
        );
        assert_eq!(
            error.observations.leader_exit,
            BoundedLeaderExitObservation::Unknown
        );
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
            &[],
            stream_limit(Instant::now() + Duration::from_millis(300), 64, 64),
            Instant::now(),
        )
        .expect_err("slow command must hit the deadline");
        assert!(matches!(error.failure, BoundedCaptureFailure::Deadline));
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
            &[],
            stream_limit(Instant::now() + Duration::from_millis(400), 64, 64),
            Instant::now(),
        )
        .expect_err("an inherited writer must not defeat the deadline");
        assert!(matches!(error.failure, BoundedCaptureFailure::Deadline));
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
            &[],
            stream_limit(Instant::now() + Duration::from_secs(30), 64, 64),
            Instant::now(),
        )
        .expect("capture within budget");
        assert_eq!(capture.status.code(), Some(3));
        assert_eq!(capture.stdout, b"out");
        assert_eq!(capture.stderr, b"err");
    }

    #[cfg(unix)]
    #[test]
    fn closed_streams_still_wait_for_exit_or_deadline() {
        let (_dir, program, args) = test_command("exec 1>&- 2>&-; exec /bin/sleep 30");
        let error = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            &[],
            stream_limit(Instant::now() + Duration::from_millis(300), 64, 64),
            Instant::now(),
        )
        .expect_err("EOF alone must not count as process completion");
        assert!(
            matches!(error.failure, BoundedCaptureFailure::Deadline),
            "{error:?}"
        );

        let (_dir, program, args) = test_command("exec 1>&- 2>&-; /bin/sleep 0.05; exit 7");
        let capture = run_command_capture_bounded(
            Path::new("."),
            &program,
            &args,
            None,
            &[],
            stream_limit(Instant::now() + Duration::from_secs(30), 64, 64),
            Instant::now(),
        )
        .expect("closed streams must preserve a later inner exit");
        assert_eq!(capture.status.code(), Some(7));
        assert!(capture.stdout.is_empty() && capture.stderr.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn inherited_writer_is_terminated_after_its_leader_exits() {
        let dir = tempfile::tempdir().expect("owned writer fixture");
        let marker = dir.path().join("writer-survived");
        // A writer that survives cancellation would create this marker, then
        // close its pipes. No guessed PID is used for cleanup or the assertion.
        let script = format!(
            "(/bin/sleep 2; printf survived > '{}') & exit 0",
            marker.display()
        );
        let (_script_owner, program, args) = test_command(&script);
        let error = run_command_capture_bounded(
            dir.path(),
            &program,
            &args,
            None,
            &[],
            stream_limit(Instant::now() + Duration::from_millis(300), 64, 64),
            Instant::now(),
        )
        .expect_err("inherited pipe must expire");
        assert!(matches!(error.failure, BoundedCaptureFailure::Deadline));
        thread::sleep(Duration::from_secs(2));
        assert!(!marker.exists(), "writer survived owned group cancellation");
    }

    #[cfg(unix)]
    #[test]
    fn cumulative_budget_is_not_reset_by_a_second_attempt() {
        // First attempt retains 3 bytes of stdout; the second attempt may only
        // retain one more before the total budget is exhausted.
        let (_dir, program, args) = test_command("printf 'abc'");
        let operation_origin = Instant::now();
        let mut budget = BoundedCommandBudget::new(
            operation_origin,
            operation_origin + Duration::from_secs(30),
            4,
            4,
        );
        let plan = plan_for(Path::new("."), &program, &args);
        budget
            .run_command(
                &plan.repo_root,
                plan.program.as_os_str(),
                &plan.args,
                None,
                &[],
                BoundedCommandPhase::CommandExecution,
            )
            .expect("first attempt");
        assert_eq!(budget.stdout_retained(), 3);
        let (_dir2, program2, args2) = test_command("printf 'de'");
        let plan2 = plan_for(Path::new("."), &program2, &args2);
        let error = budget
            .run_command(
                &plan2.repo_root,
                plan2.program.as_os_str(),
                &plan2.args,
                None,
                &[],
                BoundedCommandPhase::CommandExecution,
            )
            .expect_err("second attempt must respect the remaining budget");
        assert!(matches!(
            error,
            BoundedScopedCaptureFailure {
                failure: BoundedCaptureFailure::Overflow {
                    stream: BoundedStream::Stdout,
                    limit_bytes: 1,
                    ..
                },
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
