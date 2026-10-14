//! Read-only listener-handoff status and a checked administrator removal plan.
//!
//! Task 156 generated and privately qualified the administrator launchd
//! definition. This module adds the symmetric operator surface: a read-only
//! `effigy gateway listener-handoff-status` observation and a
//! `effigy gateway remove-listener-handoff` command that emits a *checked*
//! administrator plan. Neither command installs, activates, removes, elevates,
//! signals, or mutates anything: the emitted plan is static text for Tom to run
//! later.
//!
//! # Observation, not authority
//!
//! The definition bytes on disk are the only thing that can authenticate the
//! reviewed generator: the file is opened with `O_NOFOLLOW`, bound to a regular
//! file's identity (device, inode, size, mtime, owner, group, mode), read within
//! a fixed bound, parsed with the strict parser, and compared with the complete
//! expected generator shape plus the resolved operator and the current canonical
//! executable. launchd observations are bounded, structurally parsed results
//! from `launchctl print`; they are never treated as source identity, signal
//! permission, or an execution grant. `launchctl print` output is explicitly not
//! API (see `man launchctl`), so unrecognized structure is reported as unknown,
//! never guessed.
//!
//! # Refusal
//!
//! A removal plan is emitted only when the on-disk definition verifies as the
//! exact reviewed generator for the current operator and executable *and* the
//! launchd observation is determinate (loaded with the matching on-disk path, or
//! confirmed absent). Foreign ownership, unsafe modes, symlinks, changed bytes,
//! unknown shapes, operator or executable mismatches, permission failures,
//! capture deadlines, truncation, non-zero exits and unknown results all refuse
//! before any usable plan is produced.

// `dev_t`, `ino_t` and `mode_t` widths differ between Linux and macOS, so the
// identity casts below are redundant on one platform and required on the other.
use super::{
    parse_launchd_definition, validate_launchd_definition, HandoffIdentity, ListenerHandoffSetup,
    PlistValue, SetupDefinitionError, LAUNCHD_JOB_LABEL,
};
use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
#[cfg(any(test, target_os = "macos"))]
use std::process::{Child, Command, Stdio};
#[cfg(any(test, target_os = "macos"))]
use std::sync::mpsc;
use std::time::Duration;
#[cfg(any(test, target_os = "macos"))]
use std::time::Instant;

/// Fixed path of the administrator-installed system definition.
pub const LAUNCHD_PLIST_PATH: &str = "/Library/LaunchDaemons/dev.effigy.gateway.user.plist";

/// Absolute path of the launchd control tool used for the read-only probe.
///
/// The fixed literal path avoids a caller-controlled `PATH` selecting a
/// different program; the argv below is always fixed and never a shell.
#[cfg(target_os = "macos")]
pub const LAUNCHCTL_PATH: &str = "/bin/launchctl";

/// Maximum definition bytes this module will read.
pub const MAX_DEFINITION_BYTES: usize = 64 * 1024;

/// Maximum bytes retained from a bounded launchd capture, per stream.
pub const MAX_LAUNCHD_CAPTURE_BYTES: usize = 64 * 1024;

/// Deadline for one bounded launchd capture.
pub const LAUNCHD_CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);

/// Identity of one opened regular file, as reported by the kernel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileIdentity {
    /// Device number of the containing filesystem.
    pub device: u64,
    /// Inode number of the opened object.
    pub inode: u64,
    /// Byte size reported for the opened object.
    pub size: u64,
    /// Modification time, seconds since the epoch.
    pub modified_seconds: i64,
    /// Modification time, nanoseconds component.
    pub modified_nanoseconds: i64,
    /// Owning user ID.
    pub uid: u32,
    /// Owning group ID.
    pub gid: u32,
    /// Mode word, including file type bits.
    pub mode: u32,
    /// Whether the observed object is a regular file.
    pub regular: bool,
}

impl FileIdentity {
    /// True when the observed object is a regular file.
    pub fn is_regular(self) -> bool {
        self.regular
    }

    /// True when group or other may write the file.
    pub fn is_group_or_other_writable(self) -> bool {
        self.mode & 0o022 != 0
    }
}

/// Result of observing the fixed definition path without following symlinks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionFileReading {
    /// A regular file was opened and read within the byte bound.
    Present {
        /// Kernel identity of the opened file.
        identity: FileIdentity,
        /// Exact bytes read.
        bytes: Vec<u8>,
    },
    /// The path does not exist.
    Missing,
    /// Opening the path was refused for this user.
    PermissionDenied,
    /// The path names a symbolic link.
    Symlink,
    /// The path exists but is not a regular file.
    NotRegular,
    /// The file is larger than [`MAX_DEFINITION_BYTES`].
    Oversized,
    /// The file changed while it was being read.
    Changed,
    /// Any other open or read failure.
    Unreadable,
}

fn file_identity(metadata: &std::fs::Metadata) -> FileIdentity {
    FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.size(),
        modified_seconds: metadata.mtime(),
        modified_nanoseconds: metadata.mtime_nsec(),
        uid: metadata.uid(),
        gid: metadata.gid(),
        mode: metadata.mode(),
        regular: metadata.file_type().is_file(),
    }
}

/// Observe the definition at `path` read-only and without following symlinks.
///
/// The open binds a single inode; the result carries that inode's identity and
/// the exact bytes read within [`MAX_DEFINITION_BYTES`]. A second identity probe
/// after the read detects a concurrent change to the opened inode. `O_NOFOLLOW`
/// means a symlink refuses rather than redirects the read; `O_NONBLOCK` keeps a
/// FIFO or other non-regular object at the fixed path from blocking this
/// process before the regular-file check, so a foreign object is a bounded
/// [`DefinitionFileReading::NotRegular`] refusal, never a hang.
pub fn observe_definition_file(path: &Path) -> DefinitionFileReading {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    let file = match options.open(path) {
        Ok(file) => file,
        Err(error) => {
            return match error.raw_os_error() {
                Some(libc::ENOENT) => DefinitionFileReading::Missing,
                Some(libc::EACCES) | Some(libc::EPERM) => DefinitionFileReading::PermissionDenied,
                Some(libc::ELOOP) => DefinitionFileReading::Symlink,
                _ => DefinitionFileReading::Unreadable,
            };
        }
    };
    let before = match file.metadata() {
        Ok(metadata) => file_identity(&metadata),
        Err(_) => return DefinitionFileReading::Unreadable,
    };
    if !before.is_regular() {
        return DefinitionFileReading::NotRegular;
    }
    let mut file = file;
    let mut bytes = Vec::new();
    // `Read::take` is called on the mutable borrow so the `Take` wrapper does
    // not own (and cannot close) the underlying descriptor.
    {
        let mut reader = (&mut file).take((MAX_DEFINITION_BYTES + 1) as u64);
        if reader.read_to_end(&mut bytes).is_err() {
            return DefinitionFileReading::Unreadable;
        }
    }
    if bytes.len() > MAX_DEFINITION_BYTES {
        return DefinitionFileReading::Oversized;
    }
    let after = match file.metadata() {
        Ok(metadata) => file_identity(&metadata),
        Err(_) => return DefinitionFileReading::Unreadable,
    };
    if before != after {
        return DefinitionFileReading::Changed;
    }
    DefinitionFileReading::Present {
        identity: after,
        bytes,
    }
}

/// Observed run state of a loaded launchd job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobRunState {
    /// The job reported `state = running`.
    Running,
    /// The job reported a recognized non-running state.
    NotRunning,
    /// The job loaded but its state line was absent or unrecognized.
    Unknown,
}

/// Bounded, structurally parsed launchd observation or capture failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobObservation {
    /// The fixed label is loaded in the system domain.
    Loaded {
        /// Observed run state.
        state: JobRunState,
    },
    /// launchd reported the documented "could not find service" absence.
    ConfirmedAbsent,
    /// launchd reported a permission failure.
    PermissionDenied,
    /// The bounded capture exceeded its deadline; the child was terminated.
    TimedOut,
    /// The bounded capture exceeded its byte limit; the child was terminated.
    OutputLimit,
    /// launchd exited non-zero without a recognized absence or permission result.
    NonZero,
    /// launchd exited zero with unrecognized or malformed structure.
    Malformed,
    /// The loaded job's on-disk path is not the fixed definition path.
    AssociationMismatch,
    /// launchd could not be launched.
    Unavailable,
    /// launchd socket activation does not exist on this platform.
    Unsupported,
}

impl JobObservation {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Loaded {
                state: JobRunState::Running,
            } => "loaded_running",
            Self::Loaded {
                state: JobRunState::NotRunning,
            } => "loaded_not_running",
            Self::Loaded {
                state: JobRunState::Unknown,
            } => "loaded_state_unknown",
            Self::ConfirmedAbsent => "confirmed_absent",
            Self::PermissionDenied => "permission_denied",
            Self::TimedOut => "capture_timed_out",
            Self::OutputLimit => "capture_truncated",
            Self::NonZero => "nonzero_exit",
            Self::Malformed => "malformed",
            Self::AssociationMismatch => "association_mismatch",
            Self::Unavailable => "unavailable",
            Self::Unsupported => "unsupported",
        }
    }

    fn is_loaded(self) -> bool {
        matches!(self, Self::Loaded { .. })
    }

    fn is_determinate(self) -> bool {
        self.is_loaded() || self == Self::ConfirmedAbsent
    }
}

/// Raw outcome of one bounded, owned child capture.
#[cfg(any(test, target_os = "macos"))]
enum BoundedCapture {
    Success(Vec<u8>),
    NonZero {
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    SpawnFailed,
    TimedOut,
    OutputLimit,
}

/// One captured child stream.
#[cfg(any(test, target_os = "macos"))]
#[derive(Debug, Clone, Copy)]
enum CaptureStream {
    Stdout,
    Stderr,
}

/// Bounded result from one reader thread.
#[cfg(any(test, target_os = "macos"))]
enum StreamOutcome {
    Complete(Vec<u8>),
    Overflow,
}

#[cfg(any(test, target_os = "macos"))]
/// Read one child pipe to EOF or the byte limit on an owned reader thread.
///
/// This is the safe-std replacement for setting `O_NONBLOCK` on the pipe: the
/// reader blocks on its own thread while the caller keeps its absolute
/// deadline, so a slow or leaking child cannot block the caller.
fn read_bounded_stream<R: Read + Send + 'static>(
    stream: CaptureStream,
    mut reader: R,
    limit: usize,
    sender: mpsc::Sender<(CaptureStream, StreamOutcome)>,
) {
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let outcome = loop {
        match reader.read(&mut buffer) {
            Ok(0) => break StreamOutcome::Complete(bytes),
            Ok(read) => {
                if bytes.len() + read > limit {
                    break StreamOutcome::Overflow;
                }
                bytes.extend_from_slice(&buffer[..read]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break StreamOutcome::Complete(bytes),
        }
    };
    let _ = sender.send((stream, outcome));
}

#[cfg(any(test, target_os = "macos"))]
fn stop_owned_child(child: &mut Child) {
    // Best-effort termination and reaping of a child this call started. The
    // result is deliberately ignored: the capture outcome is already a
    // refusal and the child is never referenced again.
    let _ = child.kill();
    let _ = child.wait();
}

/// Run one owned child with piped output under an absolute deadline and byte
/// limit, reading both streams on owned threads without any unsafe I/O.
#[cfg(any(test, target_os = "macos"))]
fn capture_bounded(
    program: &str,
    args: &[&str],
    timeout: Duration,
    limit: usize,
) -> BoundedCapture {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return BoundedCapture::SpawnFailed,
    };
    let (sender, receiver) = mpsc::channel();
    let mut expected = 0_usize;
    if let Some(stdout) = child.stdout.take() {
        expected += 1;
        let sender = sender.clone();
        let _ = std::thread::spawn(move || {
            read_bounded_stream(CaptureStream::Stdout, stdout, limit, sender)
        });
    }
    if let Some(stderr) = child.stderr.take() {
        expected += 1;
        let sender = sender.clone();
        let _ = std::thread::spawn(move || {
            read_bounded_stream(CaptureStream::Stderr, stderr, limit, sender)
        });
    }
    drop(sender);

    let deadline = Instant::now() + timeout;
    let mut stdout_bytes = Vec::new();
    let mut stderr_bytes = Vec::new();
    let mut received = 0_usize;
    let mut overflow = false;
    let mut timed_out = false;
    let mut unavailable = false;

    let status = loop {
        collect_capture_results(
            &receiver,
            &mut stdout_bytes,
            &mut stderr_bytes,
            &mut received,
            &mut overflow,
        );
        if overflow {
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                timed_out = true;
                break None;
            }
            Err(_) => {
                unavailable = true;
                break None;
            }
        }
    };
    if overflow || timed_out || unavailable {
        stop_owned_child(&mut child);
    }

    // After the child is reaped both pipes reach EOF, so the reader threads
    // normally finish immediately. A foreign process that still holds a pipe
    // open is bounded by the deadline and reported as an incomplete capture.
    let drain_deadline = deadline.max(Instant::now() + Duration::from_millis(100));
    while received < expected {
        let remaining = drain_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok((stream, StreamOutcome::Complete(bytes))) => {
                received += 1;
                match stream {
                    CaptureStream::Stdout => stdout_bytes = bytes,
                    CaptureStream::Stderr => stderr_bytes = bytes,
                }
            }
            Ok((_, StreamOutcome::Overflow)) => {
                received += 1;
                overflow = true;
            }
            Err(_) => break,
        }
    }

    if overflow {
        return BoundedCapture::OutputLimit;
    }
    if unavailable {
        return BoundedCapture::SpawnFailed;
    }
    if timed_out || received < expected {
        return BoundedCapture::TimedOut;
    }
    let Some(status) = status else {
        return BoundedCapture::SpawnFailed;
    };
    if status.success() {
        BoundedCapture::Success(stdout_bytes)
    } else {
        BoundedCapture::NonZero {
            code: status.code(),
            stdout: stdout_bytes,
            stderr: stderr_bytes,
        }
    }
}

/// Move every reader result already on the channel into the capture buffers.
#[cfg(any(test, target_os = "macos"))]
fn collect_capture_results(
    receiver: &mpsc::Receiver<(CaptureStream, StreamOutcome)>,
    stdout_bytes: &mut Vec<u8>,
    stderr_bytes: &mut Vec<u8>,
    received: &mut usize,
    overflow: &mut bool,
) {
    while let Ok((stream, outcome)) = receiver.try_recv() {
        *received += 1;
        match outcome {
            StreamOutcome::Complete(bytes) => match stream {
                CaptureStream::Stdout => *stdout_bytes = bytes,
                CaptureStream::Stderr => *stderr_bytes = bytes,
            },
            StreamOutcome::Overflow => *overflow = true,
        }
    }
}

#[cfg(any(test, target_os = "macos"))]
fn looks_permission_denied(text: &str) -> bool {
    text.contains("Operation not permitted")
        || text.contains("Permission denied")
        || text.contains("Not privileged")
}

/// Parse a `launchctl print system/<label>` service document structurally.
///
/// Recognizes only the documented service header line and the top-level
/// `state`/`path` keys (single tab indentation). Any other structure — a
/// missing header, a different on-disk path, a nested state line only — is
/// reported as unknown or mismatched rather than interpreted.
#[cfg(any(test, target_os = "macos"))]
fn parse_launchd_print_service(text: &str, label: &str, expected_path: &str) -> JobObservation {
    let expected_header = format!("system/{label} = {{");
    let Some(header) = text.lines().find(|line| !line.trim().is_empty()) else {
        return JobObservation::Malformed;
    };
    if header.trim() != expected_header {
        return JobObservation::Malformed;
    }
    let mut state = None;
    let mut path = None;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("\tstate = ") {
            state = Some(value.trim());
        } else if let Some(value) = line.strip_prefix("\tpath = ") {
            path = Some(value.trim());
        }
    }
    let Some(path) = path else {
        return JobObservation::AssociationMismatch;
    };
    if path != expected_path {
        return JobObservation::AssociationMismatch;
    }
    let Some(state) = state else {
        return JobObservation::Loaded {
            state: JobRunState::Unknown,
        };
    };
    let state = match state {
        "running" => JobRunState::Running,
        "not running" | "exited" | "waiting" | "spawn scheduled" => JobRunState::NotRunning,
        _ => JobRunState::Unknown,
    };
    JobObservation::Loaded { state }
}

#[cfg(any(test, target_os = "macos"))]
fn classify_launchd_capture(
    label: &str,
    expected_path: &str,
    capture: BoundedCapture,
) -> JobObservation {
    match capture {
        BoundedCapture::SpawnFailed => JobObservation::Unavailable,
        BoundedCapture::TimedOut => JobObservation::TimedOut,
        BoundedCapture::OutputLimit => JobObservation::OutputLimit,
        BoundedCapture::NonZero {
            code,
            stderr,
            stdout,
        } => {
            let stderr = String::from_utf8_lossy(&stderr);
            if looks_permission_denied(&stderr)
                || looks_permission_denied(&String::from_utf8_lossy(&stdout))
            {
                return JobObservation::PermissionDenied;
            }
            let absent = format!("Could not find service \"{label}\" in domain for system");
            if code == Some(113)
                && stderr.lines().any(|line| line.trim() == absent)
                && stdout.is_empty()
            {
                return JobObservation::ConfirmedAbsent;
            }
            JobObservation::NonZero
        }
        BoundedCapture::Success(stdout) => {
            parse_launchd_print_service(&String::from_utf8_lossy(&stdout), label, expected_path)
        }
    }
}

/// Observe the fixed launchd job read-only.
///
/// On macOS this runs the fixed `/bin/launchctl print system/<label>` argv
/// (never a shell, never interpolated text, never a caller-controlled `PATH`)
/// through [`capture_bounded`]. Other platforms report the observation as
/// unsupported.
#[cfg(target_os = "macos")]
pub fn observe_launchd_job(expected_path: &str) -> JobObservation {
    let target = format!("system/{LAUNCHD_JOB_LABEL}");
    let args = ["print", target.as_str()];
    classify_launchd_capture(
        LAUNCHD_JOB_LABEL,
        expected_path,
        capture_bounded(
            LAUNCHCTL_PATH,
            &args,
            LAUNCHD_CAPTURE_TIMEOUT,
            MAX_LAUNCHD_CAPTURE_BYTES,
        ),
    )
}

/// Observe the fixed launchd job read-only on non-macOS.
#[cfg(not(target_os = "macos"))]
pub fn observe_launchd_job(_expected_path: &str) -> JobObservation {
    JobObservation::Unsupported
}

/// The operator and executable the current generator would produce.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorExpectation {
    /// UID that must own the administrator-installed definition.
    pub definition_owner_uid: u32,
    /// Resolved operator user name.
    pub user_name: String,
    /// Resolved operator primary group name.
    pub group_name: String,
    /// Resolved operator home directory.
    pub home: PathBuf,
    /// Canonical path of the current executable.
    pub program_path: PathBuf,
}

impl OperatorExpectation {
    /// Build the expectation for the real invoking operator.
    ///
    /// Uses the process's real UID (not the effective UID), so an elevated
    /// invocation compares against the operator who invoked it rather than
    /// silently choosing root. An elevated (`root`) or `SUDO_*` invocation is
    /// refused outright: it must not silently choose another operator, and the
    /// read-only observations do not require privilege.
    pub fn for_current_operator() -> Result<Self, SetupDefinitionError> {
        let identity = HandoffIdentity::current();
        if identity.effective_uid == 0 || identity.sudo_environment_present {
            return Err(SetupDefinitionError::OperatorRecord);
        }
        let setup = ListenerHandoffSetup::for_current_operator()?;
        Ok(Self::from_setup(&setup))
    }

    /// Derive the expectation from a validated setup definition.
    pub fn from_setup(setup: &ListenerHandoffSetup) -> Self {
        Self {
            definition_owner_uid: 0,
            user_name: setup.user_name.clone(),
            group_name: setup.group_name.clone(),
            home: setup.home.clone(),
            program_path: setup.program_path.clone(),
        }
    }
}

/// Overall disposition of the on-disk definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionState {
    /// The exact reviewed definition for this operator and executable.
    Verified,
    /// The fixed path does not exist.
    Missing,
    /// Opening the definition was refused.
    PermissionDenied,
    /// The fixed path is a symbolic link.
    Symlink,
    /// The fixed path is not a regular file.
    NotRegular,
    /// The definition exceeds the byte bound.
    Oversized,
    /// The definition changed while being read.
    Changed,
    /// Opening or reading the definition failed.
    Unreadable,
    /// The definition is not owned by the administrator.
    ForeignOwner,
    /// The definition is writable by group or other.
    UnsafeMode,
    /// The bytes are not the supported plist subset.
    Malformed,
    /// The parsed definition is not the expected generator shape.
    ShapeMismatch,
    /// The definition's operator user, group or home differs.
    OperatorMismatch,
    /// The current operator could not be resolved (elevated or unknown).
    OperatorUnresolved,
    /// The definition's executable path does not exist.
    ProgramMissing,
    /// The definition's executable path is not the current executable.
    ProgramStale,
}

impl DefinitionState {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Verified => "verified",
            Self::Missing => "missing",
            Self::PermissionDenied => "permission_denied",
            Self::Symlink => "symlink",
            Self::NotRegular => "not_regular",
            Self::Oversized => "oversized",
            Self::Changed => "changed",
            Self::Unreadable => "unreadable",
            Self::ForeignOwner => "foreign_owner",
            Self::UnsafeMode => "unsafe_mode",
            Self::Malformed => "malformed",
            Self::ShapeMismatch => "shape_mismatch",
            Self::OperatorMismatch => "operator_mismatch",
            Self::OperatorUnresolved => "operator_unresolved",
            Self::ProgramMissing => "program_missing",
            Self::ProgramStale => "program_stale",
        }
    }

    fn is_verified(self) -> bool {
        self == Self::Verified
    }
}

/// Correspondence between the definition's executable and the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgramPathState {
    /// The definition names the current canonical executable.
    Current,
    /// The definition names a different existing executable.
    Stale,
    /// The definition's executable path does not exist.
    Missing,
    /// The definition could not be read or parsed.
    Unknown,
}

impl ProgramPathState {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Stale => "stale",
            Self::Missing => "missing",
            Self::Unknown => "unknown",
        }
    }
}

/// Correspondence between the definition's operator and the current one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorMatch {
    /// User, group and home all match.
    Matching,
    /// At least one operator fact differs.
    Mismatched,
    /// The definition could not be read or parsed.
    Unknown,
}

impl OperatorMatch {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::Matching => "matching",
            Self::Mismatched => "mismatched",
            Self::Unknown => "unknown",
        }
    }
}

/// Bounded reason a removal plan is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalRefusal {
    /// The definition's owner is not the administrator.
    DefinitionForeignOwner,
    /// The definition is writable by group or other.
    DefinitionUnsafeMode,
    /// The definition changed between the two fresh observations.
    DefinitionChanged,
    /// The definition is a symbolic link.
    DefinitionSymlink,
    /// The definition is not a regular file.
    DefinitionNotRegular,
    /// The definition exceeds the byte bound.
    DefinitionOversized,
    /// The definition could not be opened or read.
    DefinitionUnreadable,
    /// Opening the definition was refused.
    DefinitionPermissionDenied,
    /// The definition bytes are malformed.
    DefinitionMalformed,
    /// The definition is not the expected generator shape.
    DefinitionShapeMismatch,
    /// The definition's operator differs.
    DefinitionOperatorMismatch,
    /// The definition's executable path does not exist.
    DefinitionProgramMissing,
    /// The definition's executable path is not the current executable.
    DefinitionProgramStale,
    /// The launchd observation was refused for permissions.
    JobPermissionDenied,
    /// The launchd capture exceeded its deadline.
    JobTimedOut,
    /// The launchd capture exceeded its byte limit.
    JobOutputLimit,
    /// The launchd capture exited non-zero without a recognized result.
    JobNonZero,
    /// The launchd output was unrecognized.
    JobMalformed,
    /// The loaded job's on-disk path is not the fixed definition path.
    JobAssociationMismatch,
    /// launchd could not be launched.
    JobUnavailable,
    /// launchd is unsupported on this platform.
    JobUnsupported,
    /// A job is loaded but no verified definition accounts for it.
    JobWithoutDefinition,
    /// The current operator or executable could not be resolved.
    OperatorUnavailable,
}

impl RemovalRefusal {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::DefinitionForeignOwner => "definition_foreign_owner",
            Self::DefinitionUnsafeMode => "definition_unsafe_mode",
            Self::DefinitionChanged => "definition_changed",
            Self::DefinitionSymlink => "definition_symlink",
            Self::DefinitionNotRegular => "definition_not_regular",
            Self::DefinitionOversized => "definition_oversized",
            Self::DefinitionUnreadable => "definition_unreadable",
            Self::DefinitionPermissionDenied => "definition_permission_denied",
            Self::DefinitionMalformed => "definition_malformed",
            Self::DefinitionShapeMismatch => "definition_shape_mismatch",
            Self::DefinitionOperatorMismatch => "definition_operator_mismatch",
            Self::DefinitionProgramMissing => "definition_program_missing",
            Self::DefinitionProgramStale => "definition_program_stale",
            Self::JobPermissionDenied => "job_permission_denied",
            Self::JobTimedOut => "job_capture_timed_out",
            Self::JobOutputLimit => "job_capture_truncated",
            Self::JobNonZero => "job_nonzero_exit",
            Self::JobMalformed => "job_malformed",
            Self::JobAssociationMismatch => "job_association_mismatch",
            Self::JobUnavailable => "job_unavailable",
            Self::JobUnsupported => "job_unsupported",
            Self::JobWithoutDefinition => "job_without_definition",
            Self::OperatorUnavailable => "operator_unavailable",
        }
    }
}

/// What the removal command may do with the current observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemovalDisposition {
    /// A checked administrator plan is available.
    PlanAvailable,
    /// No definition is installed and no job is loaded.
    NothingInstalled,
    /// Association or identity is unresolved.
    Refused(RemovalRefusal),
}

impl RemovalDisposition {
    /// Stable bounded label for human and JSON surfaces.
    pub fn label(self) -> &'static str {
        match self {
            Self::PlanAvailable => "plan_available",
            Self::NothingInstalled => "nothing_installed",
            Self::Refused(_) => "refused",
        }
    }

    /// Bounded refusal label, when refused.
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Self::Refused(reason) => Some(reason.label()),
            _ => None,
        }
    }
}

/// Complete read-only listener-handoff maintenance observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandoffMaintenanceReport {
    /// On-disk definition disposition.
    pub definition: DefinitionState,
    /// Executable path correspondence.
    pub program: ProgramPathState,
    /// Operator correspondence.
    pub operator: OperatorMatch,
    /// launchd job observation.
    pub job: JobObservation,
    /// Removal disposition.
    pub removal: RemovalDisposition,
    /// Kernel identity of the definition file, when one was opened.
    pub identity: Option<FileIdentity>,
}

fn definition_refusal(state: DefinitionState) -> RemovalRefusal {
    match state {
        DefinitionState::Verified => RemovalRefusal::DefinitionUnreadable,
        DefinitionState::Missing => RemovalRefusal::DefinitionUnreadable,
        DefinitionState::PermissionDenied => RemovalRefusal::DefinitionPermissionDenied,
        DefinitionState::Symlink => RemovalRefusal::DefinitionSymlink,
        DefinitionState::NotRegular => RemovalRefusal::DefinitionNotRegular,
        DefinitionState::Oversized => RemovalRefusal::DefinitionOversized,
        DefinitionState::Changed => RemovalRefusal::DefinitionChanged,
        DefinitionState::Unreadable => RemovalRefusal::DefinitionUnreadable,
        DefinitionState::ForeignOwner => RemovalRefusal::DefinitionForeignOwner,
        DefinitionState::UnsafeMode => RemovalRefusal::DefinitionUnsafeMode,
        DefinitionState::Malformed => RemovalRefusal::DefinitionMalformed,
        DefinitionState::ShapeMismatch => RemovalRefusal::DefinitionShapeMismatch,
        DefinitionState::OperatorMismatch => RemovalRefusal::DefinitionOperatorMismatch,
        DefinitionState::OperatorUnresolved => RemovalRefusal::OperatorUnavailable,
        DefinitionState::ProgramMissing => RemovalRefusal::DefinitionProgramMissing,
        DefinitionState::ProgramStale => RemovalRefusal::DefinitionProgramStale,
    }
}

fn job_refusal(observation: JobObservation) -> RemovalRefusal {
    match observation {
        JobObservation::Loaded { .. } => RemovalRefusal::JobAssociationMismatch,
        JobObservation::ConfirmedAbsent => RemovalRefusal::JobAssociationMismatch,
        JobObservation::PermissionDenied => RemovalRefusal::JobPermissionDenied,
        JobObservation::TimedOut => RemovalRefusal::JobTimedOut,
        JobObservation::OutputLimit => RemovalRefusal::JobOutputLimit,
        JobObservation::NonZero => RemovalRefusal::JobNonZero,
        JobObservation::Malformed => RemovalRefusal::JobMalformed,
        JobObservation::AssociationMismatch => RemovalRefusal::JobAssociationMismatch,
        JobObservation::Unavailable => RemovalRefusal::JobUnavailable,
        JobObservation::Unsupported => RemovalRefusal::JobUnsupported,
    }
}

/// Classify one definition reading against the expected operator/executable.
fn classify_definition(
    reading: &DefinitionFileReading,
    expectation: &OperatorExpectation,
) -> (
    DefinitionState,
    ProgramPathState,
    OperatorMatch,
    Option<FileIdentity>,
) {
    let (identity, bytes) = match reading {
        DefinitionFileReading::Present { identity, bytes } => (Some(*identity), bytes.as_slice()),
        DefinitionFileReading::Missing => {
            return (
                DefinitionState::Missing,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::PermissionDenied => {
            return (
                DefinitionState::PermissionDenied,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::Symlink => {
            return (
                DefinitionState::Symlink,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::NotRegular => {
            return (
                DefinitionState::NotRegular,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::Oversized => {
            return (
                DefinitionState::Oversized,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::Changed => {
            return (
                DefinitionState::Changed,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
        DefinitionFileReading::Unreadable => {
            return (
                DefinitionState::Unreadable,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                None,
            )
        }
    };
    let identity = identity.expect("present reading carries identity");

    if identity.uid != expectation.definition_owner_uid {
        return (
            DefinitionState::ForeignOwner,
            ProgramPathState::Unknown,
            OperatorMatch::Unknown,
            Some(identity),
        );
    }
    if identity.is_group_or_other_writable() {
        return (
            DefinitionState::UnsafeMode,
            ProgramPathState::Unknown,
            OperatorMatch::Unknown,
            Some(identity),
        );
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return (
            DefinitionState::Malformed,
            ProgramPathState::Unknown,
            OperatorMatch::Unknown,
            Some(identity),
        );
    };
    let parsed = match parse_launchd_definition(text) {
        Ok(parsed) => parsed,
        Err(_) => {
            return (
                DefinitionState::Malformed,
                ProgramPathState::Unknown,
                OperatorMatch::Unknown,
                Some(identity),
            )
        }
    };
    if validate_launchd_definition(&parsed).is_err() {
        return (
            DefinitionState::ShapeMismatch,
            ProgramPathState::Unknown,
            OperatorMatch::Unknown,
            Some(identity),
        );
    }

    let operator = operator_match(&parsed, expectation);
    if operator != OperatorMatch::Matching {
        return (
            DefinitionState::OperatorMismatch,
            ProgramPathState::Unknown,
            operator,
            Some(identity),
        );
    }

    let program = program_path_state(&parsed, expectation);
    let definition = match program {
        ProgramPathState::Current => DefinitionState::Verified,
        ProgramPathState::Missing => DefinitionState::ProgramMissing,
        ProgramPathState::Stale | ProgramPathState::Unknown => DefinitionState::ProgramStale,
    };
    (definition, program, operator, Some(identity))
}

fn definition_string<'a>(parsed: &'a PlistValue, key: &str) -> Option<&'a str> {
    parsed.get(key).and_then(PlistValue::as_str)
}

fn operator_match(parsed: &PlistValue, expectation: &OperatorExpectation) -> OperatorMatch {
    let user = definition_string(parsed, "UserName");
    let group = definition_string(parsed, "GroupName");
    let home = parsed
        .get("EnvironmentVariables")
        .and_then(|environment| environment.get("HOME"))
        .and_then(PlistValue::as_str);
    if user == Some(expectation.user_name.as_str())
        && group == Some(expectation.group_name.as_str())
        && home == Some(expectation.home.to_string_lossy().as_ref())
    {
        OperatorMatch::Matching
    } else {
        OperatorMatch::Mismatched
    }
}

fn program_path_state(parsed: &PlistValue, expectation: &OperatorExpectation) -> ProgramPathState {
    let program = match parsed.get("ProgramArguments") {
        Some(PlistValue::Array(arguments)) => arguments.first().and_then(PlistValue::as_str),
        _ => None,
    };
    let Some(program) = program else {
        return ProgramPathState::Unknown;
    };
    let program = Path::new(program);
    if !program.is_file() {
        return ProgramPathState::Missing;
    }
    if program == expectation.program_path.as_path() {
        ProgramPathState::Current
    } else {
        ProgramPathState::Stale
    }
}

fn removal_disposition(definition: DefinitionState, job: JobObservation) -> RemovalDisposition {
    if definition.is_verified() {
        return if job.is_determinate() {
            RemovalDisposition::PlanAvailable
        } else {
            RemovalDisposition::Refused(job_refusal(job))
        };
    }
    if definition == DefinitionState::Missing {
        return match job {
            JobObservation::ConfirmedAbsent => RemovalDisposition::NothingInstalled,
            JobObservation::Loaded { .. } | JobObservation::AssociationMismatch => {
                RemovalDisposition::Refused(RemovalRefusal::JobWithoutDefinition)
            }
            other => RemovalDisposition::Refused(job_refusal(other)),
        };
    }
    RemovalDisposition::Refused(definition_refusal(definition))
}

/// Classify one reading and one job observation into a maintenance report.
///
/// Pure: it performs no I/O, so private fixtures can exercise every branch
/// through the production classification boundary.
pub fn inspect_handoff(
    reading: &DefinitionFileReading,
    job: JobObservation,
    expectation: &OperatorExpectation,
) -> HandoffMaintenanceReport {
    let (definition, program, operator, identity) = classify_definition(reading, expectation);
    let removal = removal_disposition(definition, job);
    HandoffMaintenanceReport {
        definition,
        program,
        operator,
        job,
        removal,
        identity,
    }
}

/// Observe the fixed definition and job, then classify them.
///
/// If the current operator or executable cannot be resolved, the report
/// refuses removal with [`RemovalRefusal::OperatorUnavailable`] while still
/// reporting the on-disk and launchd observations truthfully.
pub fn observe_handoff_maintenance() -> HandoffMaintenanceReport {
    classify_observed_handoff(
        observe_definition_file(Path::new(LAUNCHD_PLIST_PATH)),
        observe_launchd_job(LAUNCHD_PLIST_PATH),
    )
}

/// Observe the fixed definition with a fresh identity recheck and classify it.
///
/// The removal command uses this so a definition that changed between the
/// checked read and the plan never yields a usable plan.
pub fn observe_handoff_maintenance_for_removal() -> HandoffMaintenanceReport {
    classify_observed_handoff(
        observe_stable_definition(Path::new(LAUNCHD_PLIST_PATH)),
        observe_launchd_job(LAUNCHD_PLIST_PATH),
    )
}

/// Classify one observed reading and job against the current operator.
fn classify_observed_handoff(
    reading: DefinitionFileReading,
    job: JobObservation,
) -> HandoffMaintenanceReport {
    classify_observed_handoff_with(reading, job, OperatorExpectation::for_current_operator())
}

/// Classify a reading and job against an operator expectation that may be
/// unresolved.
///
/// A `Present` definition whose operator cannot be resolved reports the
/// explicit [`DefinitionState::OperatorUnresolved`], never a fabricated
/// operator mismatch, so the finding stays truthful under an elevated or
/// otherwise unresolvable invocation.
fn classify_observed_handoff_with(
    reading: DefinitionFileReading,
    job: JobObservation,
    operator: Result<OperatorExpectation, SetupDefinitionError>,
) -> HandoffMaintenanceReport {
    match operator {
        Ok(expectation) => inspect_handoff(&reading, job, &expectation),
        Err(_) => {
            // Without a resolved operator we cannot bind the definition to this
            // user or executable, so no removal plan may be produced. Report the
            // raw observation labels without inventing an expectation.
            let identity = match &reading {
                DefinitionFileReading::Present { identity, .. } => Some(*identity),
                _ => None,
            };
            let definition = match &reading {
                DefinitionFileReading::Present { .. } => DefinitionState::OperatorUnresolved,
                DefinitionFileReading::Missing => DefinitionState::Missing,
                DefinitionFileReading::PermissionDenied => DefinitionState::PermissionDenied,
                DefinitionFileReading::Symlink => DefinitionState::Symlink,
                DefinitionFileReading::NotRegular => DefinitionState::NotRegular,
                DefinitionFileReading::Oversized => DefinitionState::Oversized,
                DefinitionFileReading::Changed => DefinitionState::Changed,
                DefinitionFileReading::Unreadable => DefinitionState::Unreadable,
            };
            let removal = match (definition, job) {
                (DefinitionState::Missing, JobObservation::ConfirmedAbsent) => {
                    RemovalDisposition::NothingInstalled
                }
                _ => RemovalDisposition::Refused(RemovalRefusal::OperatorUnavailable),
            };
            HandoffMaintenanceReport {
                definition,
                program: ProgramPathState::Unknown,
                operator: OperatorMatch::Unknown,
                job,
                removal,
                identity,
            }
        }
    }
}

/// Re-observe the definition and refuse if its identity changed between reads.
///
/// The removal command calls this so a definition that changes between the
/// checked read and the plan never yields a usable plan. The two observations
/// never happen concurrently with a human administrator step; the operator is
/// instructed to re-run the status immediately before running the plan.
pub fn observe_stable_definition(path: &Path) -> DefinitionFileReading {
    let first = observe_definition_file(path);
    let second = observe_definition_file(path);
    if first != second {
        return DefinitionFileReading::Changed;
    }
    second
}

/// The two fixed administrator steps of the removal plan, in safe order.
pub fn removal_plan_steps() -> [&'static str; 2] {
    [
        "sudo launchctl bootout system/dev.effigy.gateway.user",
        "sudo rm /Library/LaunchDaemons/dev.effigy.gateway.user.plist",
    ]
}

/// Render the read-only status as bounded human text.
pub fn render_status_human(report: &HandoffMaintenanceReport) -> String {
    let mut rendered = String::new();
    rendered.push_str("[ok] listener handoff status (read-only; nothing was changed)\n");
    rendered.push_str(&format!("label: {LAUNCHD_JOB_LABEL}\n"));
    rendered.push_str(&format!("definition: {}\n", report.definition.label()));
    rendered.push_str(&format!("program: {}\n", report.program.label()));
    rendered.push_str(&format!("job: {}\n", report.job.label()));
    rendered.push_str(&format!("operator: {}\n", report.operator.label()));
    match report.removal {
        RemovalDisposition::PlanAvailable => rendered.push_str(
            "removal: plan available (`effigy gateway remove-listener-handoff`); the plan is unexecuted\n",
        ),
        RemovalDisposition::NothingInstalled => {
            rendered.push_str("removal: nothing installed (no definition and no loaded job)\n")
        }
        RemovalDisposition::Refused(reason) => {
            rendered.push_str(&format!("removal: refused ({})\n", reason.label()))
        }
    }
    if let Some(identity) = report.identity {
        rendered.push_str(&format!(
            "observed file: uid {} gid {} mode {:04o} size {} inode {}\n",
            identity.uid,
            identity.gid,
            identity.mode & 0o7777,
            identity.size,
            identity.inode,
        ));
    }
    rendered.push_str(
        "launchd and file observations are read-only findings; they are not source identity, signal authority, or an execution grant.\n",
    );
    rendered
}

/// Render the removal command's bounded human output.
///
/// The plan is static text. This function never claims a gateway was stopped
/// and never implies that any step ran.
pub fn render_removal_human(report: &HandoffMaintenanceReport) -> String {
    match report.removal {
        RemovalDisposition::PlanAvailable => {
            let [bootout, remove] = removal_plan_steps();
            format!(
                "[ok] listener handoff removal plan (UNEXECUTED; this command changed nothing)\n\n\
                 This is a checked administrator plan only. Nothing ran, no gateway was stopped, \
                 and no file was removed.\n\n\
                 Step 0 (required fresh identity recheck): re-run `effigy gateway listener-handoff-status` \
                 and confirm `definition: verified`, `program: current`, `operator: matching`, and a \
                 determinate job observation. If any of these is foreign, changed, stale or unknown, \
                 stop and do not run the steps below.\n\n\
                 Step 1 (KeepAlive warning: launchd restarts the job until it is booted out):\n    {bootout}\n\n\
                 Step 2:\n    {remove}\n\n\
                 Neither step has run. After an operator-executed removal, verifying success and \
                 starting a supported gateway are operator steps; nothing restarts automatically.\n\
                 See docs/guides/063-container-system-guide.md (user gateway listener handoff).\n"
            )
        }
        RemovalDisposition::NothingInstalled => {
            "[ok] listener handoff removal: nothing installed\n\nno definition is present at the fixed path and launchd reports no such job; no plan or action is available.\n"
                .to_owned()
        }
        RemovalDisposition::Refused(reason) => format!(
            "[refused] listener handoff removal refused: {}\n\nThe definition or its loaded-job association is foreign, changed or unknown. No removal plan was produced; resolve the finding with `effigy gateway listener-handoff-status` and an administrator review first.\n",
            reason.label()
        ),
    }
}

#[cfg(test)]
#[path = "maintenance/tests.rs"]
mod tests;
