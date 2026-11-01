//! Per-run re-executed supervisor and sentinel (§5.8). A binary calls
//! `internal_entry` from main, after installing its one signal router. Engines
//! keep their settlement adapter and next-command parser; no grammar lives here.
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OpenReason,
    OperationContext, StopCause,
};
use crate::custody::{Custody, EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal};
use crate::fs::{FileName, Trust};
use crate::gate::Wait;
use crate::human::Category;
use crate::id::{ResolvedInvocation, ResourceId, RunId};
use crate::ident::{self, ProcessIdentity};
use crate::observe::Observed;
use crate::outcome::{Invocation, WorkloadOutcome};
use crate::root::StateRoot;
use crate::run::{
    self, BeginOpts, Fence, Request, RequestAction, Run, RunError, SettlementAdapter,
};
use crate::signal::RouterHandle;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(crate) mod anchor;
// Scaffolding only: no helper or workspace path may use it until the birth
// receipt protocol and its exact-owner checks have been independently reviewed.
#[allow(dead_code)]
mod birth_hash;
pub(crate) mod memory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placement {
    Inline,
    Detached,
    Guest,
}
impl Placement {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::Detached => "detached",
            Self::Guest => "guest",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientMode {
    Owning,
    Detach,
}
/// The binary a run's helpers re-exec. Helpers exec whatever the path names,
/// so rebuilding or reinstalling Effigy during a long run is ordinary. The
/// helper handshake frame carries [`HELPER_PROTOCOL`]; a binary that does not
/// speak it refuses before running anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Executable {
    path: PathBuf,
}
impl Executable {
    pub fn current() -> io::Result<Self> {
        Self::capture(&std::env::current_exe()?)
    }
    pub fn capture(path: &Path) -> io::Result<Self> {
        if !std::fs::metadata(path)?.is_file() {
            return Err(io::Error::other("binary is not a regular file"));
        }
        Ok(Self { path: path.into() })
    }
}
/// The helper handshake protocol this binary speaks. A helper started from a
/// different binary refuses a frame whose version differs. 2: a refused
/// admission carries its category alongside the rule (effigy#360).
pub const HELPER_PROTOCOL: u32 = 2;
/// Refuse a helper frame this binary cannot read, naming both versions.
fn check_protocol(version: u32) -> Result<(), RunError> {
    if version == HELPER_PROTOCOL {
        return Ok(());
    }
    Err(RunError::refused(format!(
        "Effigy was updated: this helper speaks protocol {version} and this binary speaks \
         {HELPER_PROTOCOL}, so rerun the command"
    )))
}

/// The version a helper frame declares, read before the rest of the frame is
/// deserialised. A frame whose shape changed in an update still reads as a
/// version mismatch instead of as a broken store value (F8).
fn frame_protocol(frame: &[u8]) -> Result<u32, RunError> {
    #[derive(serde::Deserialize)]
    struct Declared {
        protocol: Option<u32>,
    }
    let declared: Declared = serde_json::from_slice(frame)?;
    let version = declared.protocol.ok_or_else(|| {
        RunError::refused("this helper frame declares no protocol, so rerun the command")
    })?;
    check_protocol(version)?;
    Ok(version)
}
#[derive(Debug, Clone)]
pub struct LifecyclePlan {
    pub executable: Executable,
    pub placement: Placement,
}
#[derive(Debug)]
pub(crate) struct SentinelLease {
    pub identity: ProcessIdentity,
    pub(crate) executable: Executable,
    child: Child,
    writer: Option<ChildStdin>,
}

/// A workspace sentinel whose startup was not qualified. It has no operation
/// that can register effects or turn it into a lease after an error.
#[allow(dead_code)]
pub(crate) struct PartialSentinel {
    child: Child,
    writer: Option<ChildStdin>,
    ready: Option<std::process::ChildStdout>,
    log: std::fs::File,
}
#[cfg(all(test, feature = "fault-injection"))]
impl PartialSentinel {
    pub(crate) fn child_id(&self) -> u32 {
        self.child.id()
    }
}
impl SentinelLease {
    /// Read-only evidence that this exact retained lease still has its owner
    /// pipe and the recorded process incarnation. This is not authority to
    /// launch a workload or a guarantee that the process cannot exit later.
    pub(crate) fn verify_retained(&self) -> Result<(), RunError> {
        if self.writer.is_none()
            || ident::process_identity(self.identity.pid).verified() != Some(&self.identity)
            || !matches!(
                ident::process_state(self.identity.pid),
                Observed::Verified(
                    ident::ProcessState::Active | ident::ProcessState::Stopped,
                    _
                )
            )
        {
            return Err(RunError::unavailable(
                "retained sentinel identity unavailable",
            ));
        }
        Ok(())
    }

    #[cfg(all(test, feature = "fault-injection"))]
    pub(crate) fn writer_fd(&self) -> RawFd {
        self.writer.as_ref().expect("live writer").as_raw_fd()
    }

    pub fn close(&mut self, cx: &CleanupContext) -> Result<(), RunError> {
        self.writer.take();
        crate::fault_point!(SentinelClose)?;
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
            }
            cx.operation().check("sentinel close")?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
#[derive(Serialize, Deserialize)]
struct SentinelConfig {
    protocol: u32,
    home: PathBuf,
    #[serde(default)]
    machine: bool,
    #[serde(default)]
    machine_gates: Option<crate::root::MachineGateAddress>,
    owner: ResourceId,
    run: RunId,
    next: Vec<Vec<u8>>,
    executable: Executable,
}
/// How a helper ended before acknowledging its handshake.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HelperExit {
    Code(i32),
    Signal(i32),
    /// Still running when its handshake failed: the kernel stopped its own
    /// unacknowledged child.
    Stopped,
    /// Never started: its image was refused before spawn (a replaced
    /// startup path) or the spawn itself failed.
    NotStarted,
}
impl std::fmt::Display for HelperExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Code(code) => write!(f, "exited with code {code}"),
            Self::Signal(signal) => write!(f, "was killed by signal {signal}"),
            Self::Stopped => f.write_str("stopped answering"),
            Self::NotStarted => f.write_str("could not start"),
        }
    }
}
/// The bounded stderr tail a failed handshake reports.
const HELPER_TAIL: u64 = 4096;
/// How long a helper whose handshake failed has to finish exiting.
const HELPER_EXIT_WAIT: Duration = Duration::from_secs(2);
/// A helper's stderr until it acknowledges: an unlinked private file under
/// the state root, so an early exit keeps its reason and leaves no residue.
fn helper_log(root: &StateRoot) -> Result<std::fs::File, RunError> {
    let name = FileName::new(&format!("helper-{}-{}.log", std::process::id(), nonce()))
        .map_err(|e| RunError::broken(format!("the helper log name: {e}")))?;
    let fs = crate::store::StoreError::from;
    let (file, _) = root
        .dir()
        .open_file(
            &name,
            libc::O_RDWR | libc::O_EXCL,
            Some(0o600),
            Trust::Private,
        )
        .map_err(fs)?
        .ok_or_else(|| RunError::unavailable("helper log missing"))?;
    root.dir().remove(&name, false).map_err(fs)?;
    Ok(file)
}
/// A helper that never started: nothing ran on its behalf.
fn helper_not_started(entry: &str, cause: io::Error) -> RunError {
    RunError::unavailable(format!(
        "kernel helper `{entry}` {} ({cause})",
        HelperExit::NotStarted
    ))
}
/// Verifies the image and spawns `entry` with stderr to `log`. Refusal and
/// spawn failure are typed, never store I/O. An engine supervisor gets its
/// client's working directory and environment explicitly, never inherited.
fn spawn_helper(
    executable: &Executable,
    entry: &str,
    log: &std::fs::File,
    launch: Option<&EngineLaunch>,
) -> Result<Child, RunError> {
    let mut busy = 0;
    loop {
        let mut command = helper(executable, entry).map_err(|e| helper_not_started(entry, e))?;
        if let Some(launch) = launch {
            command
                .envs(launch.env.iter().map(|(k, v)| (k, v)))
                .current_dir(&launch.cwd);
            if let Some(handoff) = &launch.handoff {
                let raw = handoff.as_raw_fd();
                // SAFETY: `pre_exec` runs in the forked child before `exec`,
                // where `dup2` is async-signal-safe. The copy at the reserved
                // slot is the only one the supervisor keeps.
                unsafe {
                    command.pre_exec(move || {
                        // `dup2` onto the descriptor's own number is a no-op,
                        // so the flag is cleared on the slot explicitly: the
                        // supervisor must still find the handoff after exec.
                        if libc::dup2(raw, crate::custody::HANDOFF_FD) < 0
                            || libc::fcntl(crate::custody::HANDOFF_FD, libc::F_SETFD, 0) < 0
                        {
                            return Err(io::Error::last_os_error());
                        }
                        Ok(())
                    });
                }
            }
        }
        command
            .stdout(Stdio::piped())
            .stderr(log.try_clone().map_err(|e| helper_not_started(entry, e))?);
        match command.spawn() {
            // A just-written image can be briefly busy while another thread's
            // fork still holds its write descriptor.
            Err(error) if error.raw_os_error() == Some(libc::ETXTBSY) && busy < 200 => {
                busy += 1;
                std::thread::sleep(Duration::from_millis(5));
            }
            spawned => return spawned.map_err(|e| helper_not_started(entry, e)),
        }
    }
}
fn helper_tail(log: &std::fs::File) -> String {
    let Ok(len) = log.metadata().map(|m| m.len()) else {
        return String::new();
    };
    let size = len.min(HELPER_TAIL);
    let mut bytes = vec![0u8; size as usize];
    match std::os::unix::fs::FileExt::read_exact_at(log, &mut bytes, len - size) {
        Ok(()) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(_) => String::new(),
    }
}
/// The typed outcome of a failed handshake. A helper still running is this
/// process's own unreaped child, so stopping it is identity-safe (R3).
fn helper_failed(
    entry: &str,
    child: &mut Child,
    log: &std::fs::File,
    cause: io::Error,
) -> RunError {
    use std::os::unix::process::ExitStatusExt;
    let deadline = Instant::now() + HELPER_EXIT_WAIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(2)),
            _ => break None,
        }
    };
    let exit = match status.map(|s| (s.code(), s.signal())) {
        Some((Some(code), _)) => HelperExit::Code(code),
        Some((None, Some(signal))) => HelperExit::Signal(signal),
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            HelperExit::Stopped
        }
    };
    let tail = helper_tail(log);
    let tail = tail.trim_end();
    let tail = if tail.is_empty() {
        String::new()
    } else {
        format!(": {tail}")
    };
    RunError::unavailable(format!(
        "kernel helper `{entry}` {exit} before acknowledging ({cause}){tail}"
    ))
}
/// After acknowledging, a helper's stderr returns to the null device it had
/// before: a long-lived helper never grows the unlinked log.
fn release_helper_log() {
    // SAFETY: open of a static C string; dup2 onto fd 2 and close of our own fd.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
        if null >= 0 {
            libc::dup2(null, libc::STDERR_FILENO);
            libc::close(null);
        }
    }
}
fn helper_root(
    cx: &EffectContext,
    home: &Path,
    machine: bool,
    gates: Option<crate::root::MachineGateAddress>,
) -> Result<StateRoot, RunError> {
    let root = if machine {
        StateRoot::machine_effects(cx, home)
    } else {
        StateRoot::effects(cx, home)
    };
    let mut root = root.map_err(|error| RunError::Store(error.into()))?;
    if !machine {
        // A helper reopens the owner's checkout by path, so it must take the
        // owner's gate spelling or its recovery would not meet the owner's
        // locks (and any blocker on them).
        root = root.into_checkout();
    }
    if let Some(address) = gates {
        let observe = cx
            .child(Duration::from_secs(10))
            .operation()
            .observe()
            .map_err(|_| RunError::unavailable("helper machine gate observation"))?;
        root = root.restore_machine_gates(&observe, address);
    }
    Ok(root)
}
fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}
/// Builds the command that runs `entry` from `executable`. Image identity is
/// the caller's business: a durable supervisor verifies the image it records,
/// while a foreground guard re-execs whatever binary is beside the run (K1).
pub(crate) fn helper(executable: &Executable, entry: &str) -> io::Result<Command> {
    let mut command = Command::new(&executable.path);
    command
        .arg(entry)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: child-side setsid is async-signal-safe and accesses no Rust state.
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }

    Ok(command)
}
pub(crate) fn spawn_sentinel(
    run: &Run<'_>,
    root: &StateRoot,
    plan: &LifecyclePlan,
    next: &Invocation,
) -> Result<SentinelLease, RunError> {
    if plan.placement == Placement::Guest {
        return Err(RunError::refused(
            "a managed run needs host authority, and this placement is a guest",
        ));
    }
    let home = root
        .path()
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| RunError::unavailable("invalid state root"))?;
    sentinel_attempt(run, root, home, plan, next)
}
fn sentinel_attempt(
    run: &Run<'_>,
    root: &StateRoot,
    home: &Path,
    plan: &LifecyclePlan,
    next: &Invocation,
) -> Result<SentinelLease, RunError> {
    let executable = &plan.executable;
    let log = helper_log(root)?;
    // A detached owner runs in the environment its client handed over; its
    // sentinel settles the same effects, so it gets that environment too.
    // An inline owner's sentinel keeps a cleared environment.
    let inherited = (plan.placement == Placement::Detached).then(|| EngineLaunch {
        payload: Vec::new(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        env: std::env::vars_os().collect(),
        handoff: None,
        admission: None,
    });
    let mut child = spawn_helper(executable, SENTINEL, &log, inherited.as_ref())?;
    // The fault driver waits for every helper, registered or not (§12.1).
    #[cfg(all(test, feature = "fault-injection"))]
    crate::fault::note_helper(child.id());
    let mut writer = child
        .stdin
        .take()
        .ok_or_else(|| RunError::unavailable("sentinel pipe missing"))?;
    let config = SentinelConfig {
        protocol: HELPER_PROTOCOL,
        home: home.into(),
        machine: root.is_machine(),
        machine_gates: root.machine_gate_address(),
        owner: run.fence().resource.clone(),
        run: run.id(),
        next: next.argv().iter().map(|a| a.as_bytes().to_vec()).collect(),
        executable: executable.clone(),
    };
    let mut ready = child
        .stdout
        .take()
        .ok_or_else(|| RunError::unavailable("sentinel ready pipe missing"))?;
    let acknowledged = (|| {
        nonblocking(writer.as_raw_fd())?;
        write_frame(&mut writer, &config)?;
        nonblocking(ready.as_raw_fd())?;
        let mut ack = [0u8];
        read_exact(&mut ready, &mut ack)?;
        if ack != [1] {
            return Err(io::Error::other("invalid sentinel ready acknowledgement"));
        }
        Ok(())
    })();
    if let Err(error) = acknowledged {
        return Err(helper_failed(SENTINEL, &mut child, &log, error));
    }
    let identity = ident::process_identity(child.id())
        .verified()
        .cloned()
        .ok_or_else(|| RunError::unavailable("sentinel identity unavailable"))?;
    Ok(SentinelLease {
        identity,
        executable: plan.executable.clone(),
        child,
        writer: Some(writer),
    })
}

/// Opt-in workspace startup retains the exact child from spawn onwards.
/// A partial child is cleanup-only; its existence is not a birth receipt.
#[allow(dead_code)]
pub(crate) fn spawn_workspace_sentinel(
    run: &Run<'_>,
    root: &StateRoot,
    plan: &LifecyclePlan,
    next: &Invocation,
    cx: &EffectContext,
) -> Result<SentinelLease, (RunError, Option<Box<PartialSentinel>>)> {
    let prepare = || -> Result<_, RunError> {
        if plan.placement == Placement::Guest {
            return Err(RunError::refused("workspace sentinel needs host authority"));
        }
        let home = root
            .path()
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| RunError::unavailable("invalid state root"))?;
        let config = SentinelConfig {
            protocol: HELPER_PROTOCOL,
            home: home.into(),
            machine: root.is_machine(),
            machine_gates: root.machine_gate_address(),
            owner: run.fence().resource.clone(),
            run: run.id(),
            next: next.argv().iter().map(|a| a.as_bytes().to_vec()).collect(),
            executable: plan.executable.clone(),
        };
        let log = helper_log(root)?;
        let inherited = (plan.placement == Placement::Detached).then(|| EngineLaunch {
            payload: Vec::new(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            env: std::env::vars_os().collect(),
            handoff: None,
            admission: None,
        });
        let child = spawn_helper(&plan.executable, SENTINEL, &log, inherited.as_ref())?;
        Ok((
            config,
            PartialSentinel {
                child,
                writer: None,
                ready: None,
                log,
            },
        ))
    };
    let (config, mut partial) = prepare().map_err(|error| (error, None))?;
    #[cfg(all(test, feature = "fault-injection"))]
    crate::fault::note_helper(partial.child.id());
    let result = (|| -> Result<ProcessIdentity, RunError> {
        crate::fault_point!(WorkspaceSentinelAfterSpawn)?;
        partial.writer = partial.child.stdin.take();
        partial.ready = partial.child.stdout.take();
        let writer = partial
            .writer
            .as_mut()
            .ok_or_else(|| RunError::unavailable("sentinel pipe missing"))?;
        let ready = partial
            .ready
            .as_mut()
            .ok_or_else(|| RunError::unavailable("sentinel ready pipe missing"))?;
        nonblocking(writer.as_raw_fd())?;
        let op = cx.child(Duration::from_secs(30));
        write_frame_with(writer, &config, op.operation())?;
        crate::fault_point!(WorkspaceSentinelAfterConfig)?;
        nonblocking(ready.as_raw_fd())?;
        let mut ack = [0u8];
        read_exact_with(ready, &mut ack, op.operation())?;
        if ack != [1] {
            return Err(RunError::unavailable(
                "sentinel acknowledgement unavailable",
            ));
        }
        crate::fault_point!(WorkspaceSentinelAfterAck)?;
        op.operation().check("workspace sentinel acknowledgement")?;
        let identity = ident::process_identity(partial.child.id())
            .verified()
            .cloned()
            .ok_or_else(|| RunError::unavailable("sentinel identity unavailable"))?;
        op.operation().check("workspace sentinel identity")?;
        Ok(identity)
    })();
    match result {
        Ok(identity) => Ok(SentinelLease {
            identity,
            executable: plan.executable.clone(),
            child: partial.child,
            writer: partial.writer,
        }),
        Err(error) => Err((error, Some(Box::new(partial)))),
    }
}
fn context(cancel: CancelToken, budget: Duration) -> OperationContext {
    OperationContext::new(
        Deadline::At(Instant::now() + budget),
        cancel,
        Limits {
            capture_tail: 65536,
            log_rotate: 1 << 20,
            log_keep: 3,
            line_max: 65536,
            queue_bytes: 1 << 20,
            probe_concurrency: 1,
        },
        EffectClass::Mutate,
        crate::ctx::Placement::Host,
    )
}
fn write_frame(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let op = context(CancelToken::default(), Duration::from_secs(30));
    write_frame_with(writer, value, &op)
}
fn write_frame_with(
    writer: &mut impl Write,
    value: &impl Serialize,
    op: &OperationContext,
) -> io::Result<()> {
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > ResolvedInvocation::MAX_WIRE_BYTES {
        return Err(io::Error::other("helper frame too large"));
    }
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend(bytes);
    write_all_with(writer, &frame, op)
}
/// A bounded write to a nonblocking helper pipe.
fn write_all(writer: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    let op = context(CancelToken::default(), Duration::from_secs(30));
    write_all_with(writer, frame, &op)
}
fn write_all_with(writer: &mut impl Write, frame: &[u8], op: &OperationContext) -> io::Result<()> {
    let mut offset = 0;
    while offset < frame.len() {
        op.check("helper write")
            .map_err(|e| io::Error::other(format!("{e:?}")))?;
        match writer.write(&frame[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "helper pipe closed",
                ))
            }
            Ok(count) => offset += count,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn read_exact(reader: &mut impl Read, bytes: &mut [u8]) -> io::Result<()> {
    let op = context(CancelToken::default(), Duration::from_secs(30));
    read_exact_with(reader, bytes, &op)
}
fn read_exact_with(
    reader: &mut impl Read,
    bytes: &mut [u8],
    op: &OperationContext,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        op.check("helper read")
            .map_err(|e| io::Error::other(format!("{e:?}")))?;
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "helper pipe closed",
                ))
            }
            Ok(n) => offset += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn read_frame(reader: &mut impl Read) -> io::Result<Vec<u8>> {
    let mut size = [0u8; 4];
    read_exact(reader, &mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    if size > ResolvedInvocation::MAX_WIRE_BYTES {
        return Err(io::Error::other("helper frame too large"));
    }
    let mut bytes = vec![0u8; size];
    read_exact(reader, &mut bytes)?;
    Ok(bytes)
}
fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: borrowed live fd, F_GETFL observes and F_SETFL adds O_NONBLOCK.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// Main-only hidden dispatch. `None` means this is an ordinary invocation.
/// Adapter callbacks retain their existing policy, and `validate_next` is the
/// engine's parser, so recovery never invents a command grammar or a writer.
/// A supervisor launched for an engine payload is refused here: its binary
/// dispatches through `engine_entry`.
pub fn internal_entry(
    entry: &str,
    router: &mut RouterHandle,
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
) -> Option<Result<(), RunError>> {
    let result = match entry {
        "__kernel-anchor" => anchor::entry(),
        SENTINEL => sentinel(adapter, validate_next),
        SUPERVISOR => read_supervisor_config().and_then(|config| match config.work {
            Work::Engine { .. } => {
                emit(&Event::Refused {
                    category: Category::Refused,
                    detail: "this binary runs no engine supervisor".into(),
                });
                Err(RunError::refused("this binary runs no engine supervisor"))
            }
            work => supervisor(
                router,
                adapter,
                validate_next,
                (config.home, config.machine),
                config.machine_gates,
                config.executable,
                work,
            ),
        }),
        _ => return None,
    };
    report_entry_error(entry, &result);
    Some(result)
}
/// `internal_entry` for a binary whose engines also run detached supervisors
/// (§5.8). The engine owns its run from `Run::begin` to settle, so it takes
/// the process's one signal router with it (K9).
pub fn engine_entry(
    entry: &str,
    mut router: RouterHandle,
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
    engine: impl FnOnce(EngineSession, RouterHandle) -> Result<(), RunError>,
) -> Option<Result<(), RunError>> {
    let result = match entry {
        "__kernel-anchor" => anchor::entry(),
        SENTINEL => sentinel(adapter, validate_next),
        SUPERVISOR => read_supervisor_config().and_then(|config| match config.work {
            Work::Engine { payload, admission } => {
                EngineSession::open(payload, admission).and_then(|session| engine(session, router))
            }
            work => supervisor(
                &mut router,
                adapter,
                validate_next,
                (config.home, config.machine),
                config.machine_gates,
                config.executable,
                work,
            ),
        }),
        _ => return None,
    };
    report_entry_error(entry, &result);
    Some(result)
}
fn report_entry_error(entry: &str, result: &Result<(), RunError>) {
    // Before acknowledgement stderr is the parent's private log, so an early
    // failure names itself in the parent's typed error; afterwards it is null.
    if let Err(error) = result {
        let _ = writeln!(io::stderr(), "{entry}: {error}");
    }
}
fn read_supervisor_config() -> Result<SupervisorConfig, RunError> {
    // Unbuffered: no byte after the config frame may sit in a reader the
    // owning channel never sees.
    let mut input = std::fs::File::from(private_dup(libc::STDIN_FILENO)?);
    nonblocking(input.as_raw_fd())?;
    nonblocking(libc::STDOUT_FILENO)?;
    let frame = read_frame(&mut input)?;
    // The version first: a frame an update changed still names the mismatch.
    frame_protocol(&frame)?;
    let config: SupervisorConfig = serde_json::from_slice(&frame)?;
    Ok(config)
}
const SENTINEL: &str = "__kernel-sentinel";
const SUPERVISOR: &str = "__kernel-supervise";
fn sentinel(
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
) -> Result<(), RunError> {
    let mut input = std::io::stdin().lock();
    nonblocking(input.as_raw_fd())?;
    let config: serde_json::Value = serde_json::from_slice(&read_frame(&mut input)?)?;
    if config.get("memory").and_then(serde_json::Value::as_bool) == Some(true) {
        return memory::serve(config, &mut input);
    }
    // The version before the shape: an update that changed this frame still
    // reads as a version mismatch, not as a broken store value (F8).
    let declared = config
        .get("protocol")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            RunError::refused("this helper frame declares no protocol, so rerun the command")
        })?;
    check_protocol(u32::try_from(declared).unwrap_or(u32::MAX))?;
    let config: SentinelConfig = serde_json::from_value(config)?;
    let next = Invocation::validated(
        config.next.into_iter().map(OsString::from_vec).collect(),
        validate_next,
    )
    .map_err(|_| RunError::broken("invalid recovery invocation"))?;
    {
        let mut ready = std::io::stdout().lock();
        ready.write_all(&[1])?;
        ready.flush()?;
    }
    release_helper_log();
    // Only this read is intentionally open-ended: its lifetime is exactly the
    // owning run. The sole writer is CLOEXEC and lives inside Run.
    let mut byte = [0u8; 1];
    loop {
        match input.read(&mut byte) {
            Ok(0) => break,
            Ok(_) => return Err(RunError::broken("unexpected sentinel pipe payload")),
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }
            Err(e) => return Err(e.into()),
        }
    }
    let op = context(CancelToken::default(), Duration::from_secs(30));
    let cx = op
        .effects()
        .map_err(|_| RunError::broken("sentinel context"))?;
    let root = helper_root(&cx, &config.home, config.machine, config.machine_gates)?;
    if root.is_machine() && config.owner.kind() == crate::id::ResourceKind::Gateway {
        let observe = op
            .observe()
            .map_err(|_| RunError::unavailable("sentinel observe"))?;
        if let Observed::Verified(reader, _) = crate::store::Store::read(&observe, &root) {
            if matches!(run::machine_completion(&reader, &config.owner), Observed::Verified(completion, _) if completion.run == config.run)
            {
                return Ok(());
            }
        }
    }
    if matches!(
        crate::run::metadata(
            &op.observe()
                .map_err(|_| RunError::unavailable("sentinel observe"))?,
            &root,
            config.run
        ),
        Observed::Verified(
            run::RunMetadata {
                sentinel_status: run::SentinelStatus::Closed,
                ..
            },
            _
        )
    ) {
        return Ok(());
    }
    if let Observed::Verified(metadata, _) = run::metadata(
        &op.observe()
            .map_err(|_| RunError::unavailable("sentinel observe"))?,
        &root,
        config.run,
    ) {
        if metadata.sentinel.is_none()
            && matches!(
                crate::gate::Gate::probe(
                    &op.observe()
                        .map_err(|_| RunError::unavailable("sentinel observe"))?,
                    &root,
                    &config.owner
                ),
                Observed::Verified(crate::gate::GateState::Held(_), _)
            )
        {
            return Ok(());
        }
    }
    let cleanup = op
        .cleanup(Duration::from_secs(30))
        .map_err(|_| RunError::broken("cleanup overflow"))?;
    crate::fault_point!(SentinelBeforeSettle)?;
    match run::settle_interrupted_wait(
        &cx,
        &root,
        (&config.owner, config.run),
        next,
        &cleanup,
        adapter,
        Wait::Until(Instant::now() + Duration::from_secs(30)),
    ) {
        Ok(_) => {}
        Err(RunError::Gate(
            crate::gate::GateError::Busy { .. } | crate::gate::GateError::TimedOut { .. },
        )) => return Ok(()),
        Err(e) => return Err(e),
    }
    crate::fault_point!(SentinelAfterSettle)?;
    Ok(())
}
#[derive(Serialize, Deserialize)]
struct WireProcess {
    argv: Vec<Vec<u8>>,
    cwd: PathBuf,
    env: Vec<(Vec<u8>, Vec<u8>, bool)>,
    grace: u64,
    lifetime: Option<u64>,
    terminal: Terminal,
    label: String,
}
impl WireProcess {
    fn encode(spec: ProcessSpec) -> Self {
        Self {
            argv: spec.argv.iter().map(|a| a.as_bytes().to_vec()).collect(),
            cwd: spec.cwd,
            env: spec
                .env
                .into_iter()
                .map(|(k, v)| {
                    let (bytes, secret) = match v {
                        EnvValue::Plain(s) => (s.as_bytes().to_vec(), false),
                        EnvValue::Secret(s) => (s.bytes().to_vec(), true),
                    };
                    (k.as_bytes().to_vec(), bytes, secret)
                })
                .collect(),
            grace: spec.grace.as_millis().min(u128::from(u64::MAX)) as u64,
            lifetime: match spec.lifetime {
                Lifetime::Finite(d) => Some(d.as_millis().min(u128::from(u64::MAX)) as u64),
                Lifetime::Open(_) => None,
            },
            terminal: spec.terminal,
            label: spec.label,
        }
    }
    fn decode(self) -> io::Result<ProcessSpec> {
        Ok(ProcessSpec {
            argv: self.argv.into_iter().map(OsString::from_vec).collect(),
            cwd: self.cwd,
            env: self
                .env
                .into_iter()
                .map(|(k, v, secret)| {
                    Ok((
                        OsString::from_vec(k),
                        if secret {
                            EnvValue::Secret(SecretValue::new(v)?)
                        } else {
                            EnvValue::Plain(OsString::from_vec(v))
                        },
                    ))
                })
                .collect::<io::Result<_>>()?,
            grace: Duration::from_millis(self.grace),
            lifetime: self
                .lifetime
                .map(|d| Lifetime::Finite(Duration::from_millis(d)))
                .unwrap_or(Lifetime::Open(OpenReason::DevServer)),
            terminal: self.terminal,
            label: self.label,
        })
    }
}
#[derive(Serialize, Deserialize)]
struct SupervisorConfig {
    protocol: u32,
    home: PathBuf,
    #[serde(default)]
    machine: bool,
    #[serde(default)]
    machine_gates: Option<crate::root::MachineGateAddress>,
    executable: Executable,
    work: Work,
}
/// What one detached supervisor runs: kernel-resolved process specs, or an
/// engine's opaque payload that the engine itself admits and drives.
#[derive(Serialize, Deserialize)]
enum Work {
    Processes {
        invocation: Vec<u8>,
        processes: Vec<WireProcess>,
        run: RunId,
        next: Vec<Vec<u8>>,
    },
    Engine {
        payload: Vec<u8>,
        /// The engine's own admission, if its dispatcher reserved one.
        #[serde(default)]
        admission: Option<crate::run::AdmissionTicket>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Admitted {
        run: RunId,
        fence: Fence,
    },
    Active,
    Log {
        process: usize,
        bytes: Vec<u8>,
    },
    Closed {
        cleanup_complete: bool,
        workload: WorkloadOutcome,
        exit_code: i32,
    },
    /// Admission refused: the category and the rule. Nothing ran.
    Refused {
        category: Category,
        detail: String,
    },
    /// An engine supervisor's own event; the kernel never reads it.
    Engine(Vec<u8>),
}
#[derive(Debug)]
pub struct SupervisorClient {
    child: Child,
    writer: Option<ChildStdin>,
    events: std::process::ChildStdout,
    buffer: Vec<u8>,
    pub run: RunId,
    pub fence: Fence,
    root: StateRoot,
    active: bool,
    closed: bool,
}
impl SupervisorClient {
    /// Close this client's owning pipe. Without an acknowledged durable detach,
    /// the supervisor stops the run; callers can still receive events and reap it.
    pub fn close_input(&mut self) {
        self.writer.take();
    }

    /// Stop and reap this exact disposable fixture child after a failed wait.
    /// This says nothing about journal settlement or descendant processes.
    #[cfg(feature = "test-support")]
    pub fn fixture_terminate_and_reap(&mut self) -> io::Result<()> {
        self.writer.take();
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        self.child.wait().map(|_| ())
    }
    pub fn event(&mut self, cx: &OperationContext) -> io::Result<Event> {
        loop {
            if self.buffer.len() >= 4 {
                let size =
                    u32::from_be_bytes(self.buffer[..4].try_into().map_err(io::Error::other)?)
                        as usize;
                if size > ResolvedInvocation::MAX_WIRE_BYTES {
                    return Err(io::Error::other("event too large"));
                }
                if self.buffer.len() >= size + 4 {
                    let event: Event = serde_json::from_slice(&self.buffer[4..size + 4])
                        .map_err(io::Error::other)?;
                    self.buffer.drain(..size + 4);
                    if matches!(event, Event::Active) {
                        self.active = true;
                    }
                    if matches!(event, Event::Closed { .. }) {
                        self.closed = true;
                    }
                    return Ok(event);
                }
            }
            cx.check("supervisor event")
                .map_err(|e| io::Error::other(format!("{e:?}")))?;
            let mut bytes = [0u8; 8192];
            match self.events.read(&mut bytes) {
                Ok(0) => {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "supervisor event stream closed",
                    ))
                }
                Ok(n) => self.buffer.extend_from_slice(&bytes[..n]),
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }
    pub fn stop(&self, cx: &EffectContext, id: RunId) -> Result<(), RunError> {
        self.send(cx, id, RequestAction::Stop)
    }
    /// A second front-end interrupt records escalation before notifying the owner.
    pub fn escalate(&self, cx: &EffectContext, id: RunId) -> Result<(), RunError> {
        self.send(cx, id, RequestAction::Escalate)
    }
    fn send(&self, cx: &EffectContext, id: RunId, action: RequestAction) -> Result<(), RunError> {
        let cleanup = cx
            .operation()
            .cleanup(Duration::from_secs(30))
            .map_err(|_| RunError::broken("cleanup overflow"))?;
        let cx = &cleanup
            .operation()
            .effects()
            .map_err(|_| RunError::broken("stop needs effects"))?;
        let root = &self.root;
        run::request(
            cx,
            root,
            &Request {
                id,
                run: self.run,
                expected: self.fence.clone(),
                action,
            },
        )?;
        if let Ok(observe) = cx.operation().child(Duration::from_secs(1)).observe() {
            let _ = run::connect_control_socket(&observe, root, &self.fence.resource, self.run);
        }
        Ok(())
    }
    /// One framed message on the owning-client pipe, for an engine
    /// supervisor's own front-end commands. A process supervisor ignores it.
    pub fn send_frame(&mut self, bytes: &[u8]) -> io::Result<()> {
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| io::Error::other("owning pipe already released"))?;
        if bytes.len() > ResolvedInvocation::MAX_WIRE_BYTES {
            return Err(io::Error::other("engine frame too large"));
        }
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend_from_slice(bytes);
        write_all(writer, &frame)
    }
    pub fn detach(mut self, cx: &EffectContext, id: RunId) -> Result<RunId, RunError> {
        while !self.active {
            self.event(cx.operation())?;
        }
        self.send(cx, id, RequestAction::Detach)?;
        // Detach is durable before releasing owning-client liveness. The owner
        // drains pending requests before interpreting EOF on the owning pipe.
        self.writer.take();
        Ok(self.run)
    }
    pub fn wait(mut self, cx: &OperationContext) -> io::Result<()> {
        self.wait_recoverable(cx)
    }

    /// Wait without consuming the child handle when the context expires or
    /// observation fails, so its owner can retry cleanup with a fresh budget.
    pub fn wait_recoverable(&mut self, cx: &OperationContext) -> io::Result<()> {
        let mut stream_error = None;
        while !self.closed {
            if let Err(error) = self.event(cx) {
                // EOF can precede the recorded child's reap (a killed owner).
                // Release our owning pipe, then reap that exact child even
                // when no Closed event can arrive. The journal owns outcome.
                self.writer.take();
                stream_error = Some(error);
                break;
            }
        }
        loop {
            if self.child.try_wait()?.is_some() {
                return stream_error.map_or(Ok(()), Err);
            }
            cx.check("supervisor exit")
                .map_err(|e| io::Error::other(format!("{e:?}")))?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
/// One detached owner. Process specs are kernel mechanisms already resolved by
/// an engine; the kernel does not parse selectors or assemble engine workflows.
#[allow(clippy::too_many_arguments)]
pub fn start_detached(
    cx: &EffectContext,
    home: &Path,
    inv: &ResolvedInvocation,
    id: RunId,
    next: &Invocation,
    processes: Vec<ProcessSpec>,
    executable: &Executable,
    client: ClientMode,
) -> Result<SupervisorClient, RunError> {
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::refused("detached spawn needs Mutate"))?;
    let root = StateRoot::effects(cx, home).map_err(crate::store::StoreError::from)?;
    start_detached_at(cx, &root, inv, id, next, processes, executable, client)
}
/// Detached custody with an explicit checkout or machine journal.
#[allow(clippy::too_many_arguments)]
pub fn start_detached_at(
    cx: &EffectContext,
    root: &StateRoot,
    inv: &ResolvedInvocation,
    id: RunId,
    next: &Invocation,
    processes: Vec<ProcessSpec>,
    executable: &Executable,
    client: ClientMode,
) -> Result<SupervisorClient, RunError> {
    let work = Work::Processes {
        invocation: serde_json::to_vec(inv)?,
        processes: processes.into_iter().map(WireProcess::encode).collect(),
        run: id,
        next: next
            .argv()
            .iter()
            .map(|arg| arg.as_bytes().to_vec())
            .collect(),
    };
    let fence = Fence {
        resource: inv.spec().owner.clone(),
        generation: crate::id::Generation::new(0),
    };
    launch(cx, root, work, None, (id, fence), executable, client)
}

/// What an engine's detached supervisor is started with besides its binary:
/// an opaque payload, and the client's working directory and environment
/// handed over explicitly at spawn. The engine admits its own run (§5.8).
#[derive(Clone)]
pub struct EngineLaunch {
    pub payload: Vec<u8>,
    pub cwd: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    /// One short-lived reader the supervisor receives at
    /// [`crate::custody::HANDOFF_FD`] and reads once before it runs anything.
    /// It is the only route a session secret takes to an engine supervisor: it
    /// is never in this environment, argv, or any later child's spawn spec.
    pub handoff: Option<std::sync::Arc<std::os::fd::OwnedFd>>,
    /// A keyed workspace request's reservation, so an engine that admits its
    /// own run in another process can bind it (052 §8.11). It travels in the
    /// supervisor's private config frame and reaches the engine as
    /// [`EngineSession::admission`]; admission re-checks the durable
    /// reservation, so the ticket alone binds nothing.
    pub admission: Option<crate::run::AdmissionTicket>,
}
impl std::fmt::Debug for EngineLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineLaunch { payload/cwd/env/handoff/admission: [REDACTED] }")
    }
}
/// One detached engine supervisor per run, in its own session. The returned
/// client learns the run identity from the engine's admission.
pub fn start_engine(
    cx: &EffectContext,
    home: &Path,
    launch_spec: EngineLaunch,
    executable: &Executable,
    client: ClientMode,
) -> Result<SupervisorClient, RunError> {
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::refused("detached spawn needs Mutate"))?;
    let root = StateRoot::effects(cx, home).map_err(crate::store::StoreError::from)?;
    start_engine_at(cx, &root, launch_spec, executable, client)
}
/// Engine custody with an explicit journal, retained by the returned client.
pub fn start_engine_at(
    cx: &EffectContext,
    root: &StateRoot,
    launch_spec: EngineLaunch,
    executable: &Executable,
    client: ClientMode,
) -> Result<SupervisorClient, RunError> {
    let work = Work::Engine {
        payload: launch_spec.payload.clone(),
        admission: launch_spec.admission.clone(),
    };
    // Placeholder identity until the engine's `Admitted` names its run.
    let pending = (
        RunId::new(0, [0; 10])
            .map_err(|e| RunError::broken(format!("a placeholder run id: {e}")))?,
        Fence {
            resource: ResourceId::store(crate::id::Scope::Machine, "engine-supervisor")
                .map_err(|e| RunError::broken(format!("a placeholder supervisor resource: {e}")))?,
            generation: crate::id::Generation::new(0),
        },
    );
    launch(
        cx,
        root,
        work,
        Some(&launch_spec),
        pending,
        executable,
        client,
    )
}
fn launch(
    cx: &EffectContext,
    root: &StateRoot,
    work: Work,
    engine: Option<&EngineLaunch>,
    (id, fence): (RunId, Fence),
    executable: &Executable,
    client: ClientMode,
) -> Result<SupervisorClient, RunError> {
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::refused("detached spawn needs Mutate"))?;
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| {
            RunError::refused("a managed run needs host authority, and this placement is a guest")
        })?;
    cx.operation().check("start detached")?;
    let config = SupervisorConfig {
        protocol: HELPER_PROTOCOL,
        home: root.owning_directory().path().into(),
        machine: root.is_machine(),
        machine_gates: root.machine_gate_address(),
        executable: executable.clone(),
        work,
    };
    let log = helper_log(root)?;
    let mut child = spawn_helper(executable, SUPERVISOR, &log, engine)?;
    let mut writer = child
        .stdin
        .take()
        .ok_or_else(|| RunError::unavailable("owner pipe missing"))?;
    let events = child
        .stdout
        .take()
        .ok_or_else(|| RunError::unavailable("event pipe missing"))?;
    let sent = (|| {
        nonblocking(writer.as_raw_fd())?;
        write_frame(&mut writer, &config)?;
        nonblocking(events.as_raw_fd())
    })();
    if let Err(error) = sent {
        return Err(helper_failed(SUPERVISOR, &mut child, &log, error));
    }
    let mut result = SupervisorClient {
        child,
        writer: Some(writer),
        events,
        buffer: Vec::new(),
        run: id,
        fence,
        root: root.clone(),
        active: false,
        closed: false,
    };
    match result.event(cx.operation()) {
        Ok(Event::Admitted { run, fence }) => {
            result.run = run;
            result.fence = fence;
        }
        Ok(Event::Refused { category, detail }) => {
            // The supervisor refused to admit its run and said why, with the
            // category it read to a person. Nothing ran on its behalf, and its
            // sentence is printed as it stands.
            result.writer.take();
            let _ = helper_failed(
                SUPERVISOR,
                &mut result.child,
                &log,
                io::Error::other("admission refused"),
            );
            return Err(RunError::reported(category, detail));
        }
        Ok(_) => {
            result.writer.take();
            let _ = helper_failed(
                SUPERVISOR,
                &mut result.child,
                &log,
                io::Error::other("invalid admission event"),
            );
            return Err(RunError::broken("invalid admission event"));
        }
        Err(error) => return Err(helper_failed(SUPERVISOR, &mut result.child, &log, error)),
    }
    // Explicit detach is an operation, not a different topology. The caller
    // supplies the durable request id through detach; Owning is the default.
    if client == ClientMode::Detach {
        while !result.active {
            result.event(cx.operation())?;
        }
        result.send(cx, result.run, RequestAction::Detach)?;
        result.writer.take();
    }
    Ok(result)
}
fn peer_owned(socket: &UnixStream) -> bool {
    #[cfg(target_os = "macos")]
    {
        let mut uid = 0;
        let mut gid = 0;
        // SAFETY: live socket and writable credential outputs.
        unsafe {
            libc::getpeereid(socket.as_raw_fd(), &mut uid, &mut gid) == 0 && uid == libc::geteuid()
        }
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: zeroed ucred and writable length match getsockopt's ABI.
        let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of_val(&cred) as libc::socklen_t;
        // SAFETY: live socket and output buffer of exactly size bytes.
        unsafe {
            libc::getsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut cred as *mut libc::ucred).cast(),
                &mut size,
            ) == 0
                && size as usize == std::mem::size_of_val(&cred)
                && cred.uid == libc::geteuid()
        }
    }
}
fn emit(event: &Event) {
    emit_to(libc::STDOUT_FILENO, event)
}
fn emit_to(fd: RawFd, event: &Event) {
    if let Ok(bytes) = serde_json::to_vec(event) {
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend(bytes);
        if frame.len() <= 4096 {
            // SAFETY: immutable initialized frame and a live event fd. One
            // nonblocking write <= PIPE_BUF is atomic; backpressure never
            // detaches a writer or changes workload outcome. Redacted rotating
            // logs retain output.
            unsafe { libc::write(fd, frame.as_ptr().cast(), frame.len()) };
        }
    }
}

/// What one turn of the owner loop learned from its signals, its requests,
/// its control socket and its owning client.
#[derive(Debug, Default)]
pub struct Control {
    /// Signals the owner forwards to its workloads.
    pub forward: Vec<i32>,
    /// Why the run must stop now, if it must.
    pub stop: Option<StopCause>,
    /// A second interrupt or an `Escalate` request: KILL now, settle after.
    pub escalate: bool,
    /// Framed messages the owning client sent its engine.
    pub frames: Vec<Vec<u8>>,
}
/// The owning client's side of a detached run: its pipe, the run's control
/// socket doorbell and the detach state. One implementation serves the
/// kernel's process supervisor and every engine supervisor.
struct OwningChannel {
    input: std::fs::File,
    buffer: Vec<u8>,
    listener: Option<std::os::unix::net::UnixListener>,
    detached: bool,
}
impl OwningChannel {
    fn turn(
        &mut self,
        run: &mut Run<'_>,
        routed: &[crate::signal::RoutedSignal],
    ) -> Result<Control, RunError> {
        let mut control = Control::default();
        for signal in routed {
            control.forward.push(signal.forward);
            control.escalate |= signal.escalate;
            control
                .stop
                .get_or_insert(StopCause::Signal(signal.received));
        }
        // EOF is read before the durable requests: a client commits its
        // detach before closing the pipe, so a request snapshot taken first
        // could miss that detach and mistake it for owner death.
        let (eof, requests) =
            observe_client_requests(&mut self.input, &mut self.buffer, || run.requests())?;
        for request in requests {
            match request.action {
                RequestAction::Detach => self.detached = true,
                RequestAction::Escalate => {
                    control.escalate = true;
                    control.stop.get_or_insert(StopCause::Requested);
                }
                RequestAction::Stop | RequestAction::Settle => {
                    control.stop.get_or_insert(StopCause::Requested);
                }
            }
            // Detach changes only this authenticated owning channel. Its
            // committed request remains the durable fact until close; repeated
            // observation is idempotent and needs no acknowledgement write.
            if !run.machine_lifecycle() {
                run.resolve_request(&request)?;
            }
        }
        if let Some(listener) = &self.listener {
            // The socket is a doorbell: the journal already holds the request.
            while let Ok((socket, _)) = listener.accept() {
                if peer_owned(&socket) {
                    drop(socket);
                }
            }
        }
        self.detached |= run.workspace_owner_detached()?;
        if eof && !self.detached {
            control.stop.get_or_insert(StopCause::OwnerLost);
        }
        while self.buffer.len() >= 4 {
            let size = u32::from_be_bytes(
                self.buffer[..4]
                    .try_into()
                    .map_err(|_| RunError::broken("client frame"))?,
            ) as usize;
            if size > ResolvedInvocation::MAX_WIRE_BYTES {
                // A malformed client stream is a lost owning client.
                self.buffer.clear();
                control.stop.get_or_insert(StopCause::OwnerLost);
                break;
            }
            if self.buffer.len() < size + 4 {
                break;
            }
            control.frames.push(self.buffer[4..size + 4].to_vec());
            self.buffer.drain(..size + 4);
        }
        Ok(control)
    }
}
/// An engine's detached supervisor (§5.8): the owning-client channel, the
/// event pipe and this process's own output. The engine admits its run with
/// `Run::begin`, then calls `admitted` before its first workload effect.
pub struct EngineSession {
    payload: Vec<u8>,
    admission: Option<crate::run::AdmissionTicket>,
    events: std::os::fd::OwnedFd,
    channel: OwningChannel,
    output: Option<OutputCapture>,
    output_disabled: bool,
    attachments: Vec<(run::Attachment, OwnerWatch)>,
}
impl std::fmt::Debug for EngineSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineSession")
            .field("detached", &self.channel.detached)
            .finish_non_exhaustive()
    }
}
impl EngineSession {
    /// Moves the owning pipe and the event pipe off descriptors 0 and 1, so
    /// nothing the engine runs can read a client frame or write an event.
    fn open(
        payload: Vec<u8>,
        admission: Option<crate::run::AdmissionTicket>,
    ) -> Result<Self, RunError> {
        let input = private_dup(libc::STDIN_FILENO)?;
        let events = private_dup(libc::STDOUT_FILENO)?;
        null_onto(libc::STDIN_FILENO)?;
        null_onto(libc::STDOUT_FILENO)?;
        Ok(Self {
            payload,
            admission,
            events,
            channel: OwningChannel {
                input: std::fs::File::from(input),
                buffer: Vec::new(),
                listener: None,
                detached: false,
            },
            output: None,
            output_disabled: false,
            attachments: Vec::new(),
        })
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
    /// The workspace request this engine must admit under, when its dispatcher
    /// reserved one: pass it to `Run::begin_admitted` (052 §8.11). `None` is an
    /// ordinary engine admission with no keyed request.
    pub fn admission(&self) -> Option<&crate::run::AdmissionTicket> {
        self.admission.as_ref()
    }
    fn emit(&self, event: &Event) {
        emit_to(self.events.as_raw_fd(), event);
    }
    /// The run is admitted: bind its control socket (K3: after `Admitted`),
    /// announce it to the owning client and return stderr to the null device.
    pub fn admitted(&mut self, run: &mut Run<'_>) -> Result<(), RunError> {
        let listener = run.control_socket()?;
        self.channel.listener = Some(listener);
        self.emit(&Event::Admitted {
            run: run.id(),
            fence: run.fence().clone(),
        });
        release_helper_log();
        Ok(())
    }
    /// Admission refused: the client reads the category's own sentence around
    /// `detail`; nothing ran. The sentence also reaches the client's helper log
    /// tail, so a refusal too long for one event frame still names itself.
    pub fn refused(&self, category: Category, detail: &str) {
        self.refused_message(category, category.render("the run", detail));
    }

    /// Admission refused with a sentence this process already rendered (its own
    /// `RunError`, or a supervisor's refusal): the client prints it verbatim,
    /// so a refusal is framed exactly once.
    pub fn refused_message(&self, category: Category, message: impl Into<String>) {
        let message = message.into();
        let _ = writeln!(io::stderr(), "{message}");
        self.emit(&Event::Refused {
            category,
            detail: effigy_ui::human::bound_text(&message, effigy_ui::human::RELAY_MAX_BYTES),
        });
    }
    pub fn active(&self) {
        self.emit(&Event::Active);
    }
    /// An engine event for the owning client. Delivery is best effort, never
    /// an outcome (K10): front ends recover state from the journal.
    pub fn event(&self, bytes: Vec<u8>) {
        self.emit(&Event::Engine(bytes));
    }
    /// One owner-loop turn over `routed` signals, pending requests, the
    /// control socket and the owning client.
    pub fn control(
        &mut self,
        run: &mut Run<'_>,
        routed: &[crate::signal::RoutedSignal],
    ) -> Result<Control, RunError> {
        // Observe client loss independently of stream reads and backpressure.
        let mut index = 0;
        while index < self.attachments.len() {
            let (attachment, watch) = &mut self.attachments[index];
            if !run.workspace_attachment_held(attachment)? {
                self.attachments.remove(index);
            } else if watch.lost() {
                run.release_workspace_attachment(attachment)?;
                self.attachments.remove(index);
            } else {
                index += 1;
            }
        }
        self.channel.turn(run, routed)
    }
    /// Watch an authenticated client's liveness pipe in the existing supervisor.
    /// EOF releases this attachment on the next control turn, even when its
    /// terminal process is killed. Only owner loss requests stop. The caller
    /// must retain the sole write end in that client, independently of data flow.
    /// Registration borrows the handle, so a failed watch leaves it available
    /// for explicit release or retry.
    pub fn watch_attachment(
        &mut self,
        run: &Run<'_>,
        attachment: &run::Attachment,
        input: &std::fs::File,
    ) -> Result<(), RunError> {
        if !run.workspace_attachment_attached(attachment)? {
            return Err(RunError::refused("this attachment was released"));
        }
        if self.attachments.len() > run::SECONDARY_MAX
            || self
                .attachments
                .iter()
                .any(|(held, _)| held.id() == attachment.id())
        {
            return Err(RunError::refused(
                "this supervisor already watches this attachment or is full",
            ));
        }
        self.attachments
            .push((attachment.lifetime_handle(), OwnerWatch::start(input)?));
        Ok(())
    }
    /// Whether the owning client detached: its death no longer stops the run.
    pub fn detached(&self) -> bool {
        self.channel.detached
    }
    /// Watches the owning pipe while the engine is inside a blocking step and
    /// cannot take a `control` turn. It reads nothing: frames stay queued.
    pub fn owner_watch(&self) -> io::Result<OwnerWatch> {
        OwnerWatch::start(&self.channel.input)
    }
    /// From now on this process's own stdout and stderr, and those of every
    /// child that inherits them, are the run's output log (sequence 0),
    /// redacted before the sink (K12). Readers use `run::read_log`.
    pub fn capture_output(
        &mut self,
        run: &mut Run<'_>,
        secrets: Vec<Vec<u8>>,
    ) -> io::Result<crate::custody::CaptureState> {
        const REASON: &str = "sanitized supervisor output capture is unavailable; inspect log storage and the classified-secret replacement policy before restarting";
        if self.output_disabled {
            return Ok(crate::custody::CaptureState::Truncated(REASON.into()));
        }
        if self.output.is_some() {
            return Ok(crate::custody::CaptureState::Complete);
        }
        match OutputCapture::start(run, secrets) {
            Ok(output) => {
                self.output = Some(output);
                Ok(crate::custody::CaptureState::Complete)
            }
            Err(_) => {
                // Capture setup may have replaced only one descriptor, or
                // created pipes before thread startup failed. No setup/workload
                // runs until BOTH descriptors are known safe and failure is
                // durably observable. No raw fallback is permitted.
                null_onto(libc::STDOUT_FILENO)?;
                null_onto(libc::STDERR_FILENO)?;
                self.output_disabled = true;
                run.record_capture_failure(REASON)
                    .map_err(|e| io::Error::other(e.to_string()))?;
                Ok(crate::custody::CaptureState::Truncated(REASON.into()))
            }
        }
    }
    /// Announces the settled outcome after the captured output is flushed.
    pub fn closed(&mut self, outcome: &crate::outcome::Outcome<crate::run::Settled>) {
        if let Some(output) = self.output.take() {
            output.finish();
        }
        self.emit(&Event::Closed {
            cleanup_complete: outcome.cleanup() == crate::outcome::CleanupOutcome::Complete,
            workload: outcome.workload().clone(),
            exit_code: outcome.exit_code(),
        });
    }
}
/// Whether the owning client went away, observed without consuming its
/// frames: the pipe polls readable with nothing left to read.
#[derive(Debug)]
pub struct OwnerWatch {
    lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl OwnerWatch {
    fn start(input: &std::fs::File) -> io::Result<Self> {
        let fd = private_dup(input.as_raw_fd())?;
        let lost = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (thread_lost, thread_done) = (lost.clone(), done.clone());
        let thread = std::thread::Builder::new()
            .name("effigy-owner-watch".into())
            .spawn(move || {
                use std::sync::atomic::Ordering;
                while !thread_done.load(Ordering::Acquire) {
                    let mut poll = libc::pollfd {
                        fd: fd.as_raw_fd(),
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: one initialized pollfd for a live descriptor.
                    let ready = unsafe { libc::poll(&mut poll, 1, 100) };
                    if ready <= 0 {
                        continue;
                    }
                    let mut pending: libc::c_int = 0;
                    // SAFETY: FIONREAD writes the readable byte count to `pending`.
                    let queued =
                        unsafe { libc::ioctl(fd.as_raw_fd(), libc::FIONREAD, &mut pending) };
                    if queued == 0 && pending == 0 {
                        thread_lost.store(true, Ordering::Release);
                        return;
                    }
                    // Queued frames wait for the owner's next turn.
                    std::thread::sleep(Duration::from_millis(50));
                }
            })?;
        Ok(Self {
            lost,
            done,
            thread: Some(thread),
        })
    }
    pub fn lost(&self) -> bool {
        self.lost.load(std::sync::atomic::Ordering::Acquire)
    }
}
impl Drop for OwnerWatch {
    fn drop(&mut self) {
        self.done.store(true, std::sync::atomic::Ordering::Release);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
/// The run's own output log, fed from pipes on descriptors 1 and 2.
struct OutputCapture {
    thread: std::thread::JoinHandle<()>,
    done: Arc<std::sync::atomic::AtomicBool>,
}
/// The output log's journal position: no effect has sequence 0.
pub const OUTPUT_SEQUENCE: i64 = 0;
impl OutputCapture {
    fn start(run: &Run<'_>, secrets: Vec<Vec<u8>>) -> io::Result<Self> {
        let limits = run.limits();
        let log = run
            .capture_log(OUTPUT_SEQUENCE, crate::custody::Stream::Stdout, &limits)?
            .ok_or_else(|| io::Error::other("managed supervisor output needs a durable log"))?;
        let output = crate::custody::CanonicalOutput::for_run(log, secrets, &limits)?;
        if let crate::custody::CaptureState::Truncated(reason) = output.capture_state() {
            return Err(io::Error::other(reason));
        }
        let [(stdout, out_write), (stderr, err_write)] = [
            crate::custody::spawn::pipe()?,
            crate::custody::spawn::pipe()?,
        ];
        for (target, write) in [
            (libc::STDOUT_FILENO, &out_write),
            (libc::STDERR_FILENO, &err_write),
        ] {
            // SAFETY: dup2 of a live pipe end onto a standard descriptor.
            if unsafe { libc::dup2(write.as_raw_fd(), target) } < 0 {
                return Err(io::Error::last_os_error());
            }
            if target == libc::STDOUT_FILENO {
                crate::fault_point!(CaptureSetupAfterStdout)?;
            }
        }
        let readers = [stdout, stderr];
        let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopping = done.clone();
        crate::fault_point!(CaptureSetupBeforeThread)?;
        let thread = std::thread::Builder::new()
            .name("effigy-run-output".into())
            .spawn(move || pump_output(readers, output, stopping))?;
        Ok(Self { thread, done })
    }
    /// The owner's settle has closed its children. Bound a remaining inherited
    /// pipe holder: no frontend or reader can keep supervisor cleanup alive.
    fn finish(self) {
        let _ = null_onto(libc::STDOUT_FILENO);
        let _ = null_onto(libc::STDERR_FILENO);
        self.done.store(true, std::sync::atomic::Ordering::Release);
        let _ = self.thread.join();
    }
}
/// Own both pipe reads and their one durable writer. A stopped owner cannot
/// wait forever for a foreign inherited descriptor to reach EOF.
fn pump_output(
    readers: [std::os::fd::OwnedFd; 2],
    output: crate::custody::CanonicalOutput,
    stopping: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut open = [true, true];
    let mut deadline = None;
    let mut buf = [0u8; 8192];
    while open.iter().any(|open| *open) {
        if stopping.load(std::sync::atomic::Ordering::Acquire) {
            let until = deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(1));
            if Instant::now() >= *until {
                output.fail();
                break;
            }
        }
        let mut fds: [libc::pollfd; 2] = std::array::from_fn(|index| libc::pollfd {
            fd: if open[index] {
                readers[index].as_raw_fd()
            } else {
                -1
            },
            events: libc::POLLIN,
            revents: 0,
        });
        // SAFETY: initialized pollfd array of its own length.
        if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 50) } < 0 {
            if io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
                continue;
            }
            output.fail();
            break;
        }
        for (index, fd) in fds.iter().enumerate() {
            if !open[index] || fd.revents == 0 {
                continue;
            }
            // SAFETY: live pipe read end and writable stack buffer.
            let count = unsafe { libc::read(fd.fd, buf.as_mut_ptr().cast(), buf.len()) };
            if count == 0 {
                open[index] = false;
                continue;
            }
            if count < 0 {
                let error = io::Error::last_os_error();
                if !matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) {
                    output.fail();
                    open[index] = false;
                }
                continue;
            }
            if output.feed(index, &buf[..count as usize], false).is_err() {
                output.fail();
            }
        }
    }
    let _ = output.finish();
}
fn private_dup(fd: RawFd) -> io::Result<std::os::fd::OwnedFd> {
    // SAFETY: duplicates a live standard descriptor to a fresh CLOEXEC fd.
    let copy = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if copy < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fcntl returned a new fd owned exactly once here.
    Ok(unsafe { std::os::fd::OwnedFd::from_raw_fd(copy) })
}
fn null_onto(target: RawFd) -> io::Result<()> {
    // SAFETY: open of a static C string, dup2 onto a standard descriptor and
    // close of our own temporary fd.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC);
        if null < 0 {
            return Err(io::Error::last_os_error());
        }
        let result = libc::dup2(null, target);
        libc::close(null);
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

fn supervisor(
    router: &mut RouterHandle,
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
    (home, machine): (PathBuf, bool),
    machine_gates: Option<crate::root::MachineGateAddress>,
    executable: Executable,
    work: Work,
) -> Result<(), RunError> {
    let Work::Processes {
        invocation,
        processes,
        run: run_id,
        next,
    } = work
    else {
        return Err(RunError::refused("process supervisor work"));
    };
    let inv = ResolvedInvocation::decode(&invocation)
        .map_err(|e| RunError::broken(format!("the invocation on the wire: {e}")))?;
    let next = Invocation::validated(
        next.into_iter().map(OsString::from_vec).collect(),
        validate_next,
    )
    .map_err(|_| RunError::broken("invalid recovery invocation"))?;
    let op = OperationContext::new(
        Deadline::Open(OpenReason::DevServer),
        router.root_cancel(),
        context(CancelToken::default(), Duration::from_secs(30))
            .limits()
            .clone(),
        EffectClass::Mutate,
        crate::ctx::Placement::Host,
    );
    let cx = op
        .effects()
        .map_err(|_| RunError::broken("supervisor context"))?;
    let root = helper_root(&cx, &home, machine, machine_gates)?;
    let cleanup = op
        .cleanup(Duration::from_secs(30))
        .map_err(|_| RunError::broken("cleanup overflow"))?;
    let specs = processes
        .into_iter()
        .map(WireProcess::decode)
        .collect::<io::Result<Vec<_>>>()?;
    let options = BeginOpts {
        id: run_id,
        wait: Wait::FailFast,
        acknowledge: vec![],
        next,
        custody: Some(LifecyclePlan {
            executable,
            placement: Placement::Detached,
        }),
    };
    let run = if root.is_machine() && inv.spec().owner.kind() == crate::id::ResourceKind::Gateway {
        let [process] = specs.as_slice() else {
            return Err(RunError::refused("gateway must have one planned process"));
        };
        Run::begin_gateway(&cx, &root, &inv, options, &cleanup, adapter, process)
    } else {
        Run::begin(&cx, &root, &inv, options, &cleanup, adapter)
    };
    let mut run = match run {
        Ok(run) => run,
        Err(error) => {
            emit(&Event::Refused {
                category: error.category(),
                detail: error.to_string(),
            });
            return Err(error);
        }
    };
    let listener = match run.control_socket() {
        Ok(listener) => listener,
        Err(error) => {
            emit(&Event::Refused {
                category: error.category(),
                detail: error.to_string(),
            });
            release_helper_log();
            run.interrupt(&cleanup, WorkloadOutcome::NotStarted)?;
            run.settle(&cleanup, adapter)?;
            return Ok(());
        }
    };
    emit(&Event::Admitted {
        run: run.id(),
        fence: run.fence().clone(),
    });
    release_helper_log();
    let mut channel = OwningChannel {
        input: std::fs::File::from(private_dup(libc::STDIN_FILENO)?),
        buffer: Vec::new(),
        listener: Some(listener),
        detached: false,
    };
    let mut children = match Custody::spawn_all(&mut run, specs, &cx) {
        Ok(children) => children,
        Err(error) => {
            let result = run.settle(&cleanup, adapter);
            emit(&Event::Refused {
                category: Category::Unavailable,
                detail: format!("a workload spawn failed ({error:?})"),
            });
            result?;
            return Ok(());
        }
    };
    run.advance(run::Phase::Active)?;
    emit(&Event::Active);
    let mut seen = vec![false; children.len()];
    'owner: loop {
        let routed = router.poll();
        let control = channel.turn(&mut run, &routed)?;
        for signal in &control.forward {
            for child in &mut children {
                child.forward(*signal);
            }
        }
        if control.escalate {
            for child in &mut children {
                child.forward(libc::SIGKILL);
            }
        }
        let stop = op.cancel_token().cause().cloned().or(control.stop);
        if let Some(cause) = stop {
            let _ = run.interrupt(
                &op.cleanup(Duration::from_secs(30))
                    .map_err(|_| RunError::broken("cleanup overflow"))?,
                WorkloadOutcome::Cancelled {
                    signal: match &cause {
                        StopCause::Signal(signal) => Some(*signal),
                        _ => None,
                    },
                    cause,
                },
            );
            break;
        }
        let mut done = true;
        let mut code = 0;
        for (index, child) in children.iter_mut().enumerate() {
            if child.expired() {
                run.interrupt(
                    &op.cleanup(Duration::from_secs(30))
                        .map_err(|_| RunError::broken("cleanup overflow"))?,
                    WorkloadOutcome::TimedOut {
                        phase: "workload".into(),
                    },
                )?;
                break 'owner;
            }
            match child.poll() {
                Observed::Verified(exit, _) => {
                    if !seen[index] {
                        child.complete(&mut run, exit)?;
                        seen[index] = true;
                    }
                    if exit.code != 0 {
                        code = exit.code;
                    }
                }
                _ => done = false,
            }
        }
        for (index, child) in children.iter().enumerate() {
            let output = child.take_output();
            for bytes in output.chunks(512) {
                emit(&Event::Log {
                    process: index,
                    bytes: bytes.to_vec(),
                });
            }
        }
        if done {
            run.finish(if code == 0 {
                WorkloadOutcome::Succeeded
            } else {
                WorkloadOutcome::Failed { code }
            })?;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let result = run.settle(
        &op.cleanup(Duration::from_secs(30))
            .map_err(|_| RunError::broken("cleanup overflow"))?,
        adapter,
    )?;
    emit(&Event::Closed {
        cleanup_complete: result.cleanup() == crate::outcome::CleanupOutcome::Complete,
        workload: result.workload().clone(),
        exit_code: result.exit_code(),
    });
    Ok(())
}

#[cfg(all(test, feature = "fault-injection"))]
mod tests;
// EOF is causally after the client's last durable request. Keep the pipe read
// and the request read together, in that order, so detach cannot be mistaken
// for owner death.
fn observe_client_requests(
    input: &mut dyn Read,
    buffer: &mut Vec<u8>,
    requests: impl FnOnce() -> Result<Vec<Request>, RunError>,
) -> Result<(bool, Vec<Request>), RunError> {
    let mut bytes = [0u8; 8192];
    let mut eof = false;
    loop {
        match input.read(&mut bytes) {
            Ok(0) => {
                eof = true;
                break;
            }
            Ok(count) => buffer.extend_from_slice(&bytes[..count]),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    Ok((eof, requests()?))
}

#[cfg(test)]
mod client_order_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn detach_committed_at_eof_is_included_before_owner_loss_decision() {
        struct Eof<'a>(&'a Cell<bool>);
        impl Read for Eof<'_> {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                // The client commits detach immediately before closing its
                // pipe, after any earlier request snapshot could have returned.
                self.0.set(true);
                Ok(0)
            }
        }
        let committed = Cell::new(false);
        let id = RunId::new(1, [1; 10]).unwrap();
        let (eof, requests) =
            observe_client_requests(&mut Eof(&committed), &mut Vec::new(), || {
                Ok(if committed.get() {
                    vec![Request {
                        id,
                        run: id,
                        expected: Fence {
                            resource: ResourceId::gateway("fixture").unwrap(),
                            generation: crate::id::Generation::new(1),
                        },
                        action: RequestAction::Detach,
                    }]
                } else {
                    vec![]
                })
            })
            .unwrap();
        assert!(eof);
        assert!(matches!(
            requests.as_slice(),
            [Request {
                action: RequestAction::Detach,
                ..
            }]
        ));
    }
}
