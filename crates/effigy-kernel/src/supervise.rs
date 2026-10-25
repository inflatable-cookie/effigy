//! Per-run re-executed supervisor and sentinel (§5.8). A binary calls
//! `internal_entry` from main, after installing its one signal router. Engines
//! keep their settlement adapter and next-command parser; no grammar lives here.
use crate::ctx::{
    CancelToken, CleanupContext, Deadline, EffectClass, EffectContext, Limits, OpenReason,
    OperationContext, StopCause,
};
use crate::custody::{Custody, EnvValue, Lifetime, ProcessSpec, SecretValue, Terminal};
use crate::fs::{DirFd, FileName, Trust};
use crate::gate::Wait;
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
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

pub(crate) mod anchor;
mod image;
pub(crate) mod memory;
use image::{private_image, PrivateImage};

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
/// The binary a run's helpers re-exec, identified by its file: device,
/// inode, size and modification and change times. Any replacement or
/// rewrite changes at least one, and the change time cannot be set back by
/// the writer (I-SV4). Identification is a stat, so a large debug build costs
/// nothing per run. The capturing process also holds the startup image open,
/// so a rebuild that replaces the path (a `cargo build` of the running binary)
/// still leaves that image to re-exec.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Executable {
    path: PathBuf,
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    #[serde(skip)]
    image: Option<std::sync::Arc<std::fs::File>>,
    // Only the private, parent-to-helper frame carries this inherited fd.
    #[cfg(target_os = "linux")]
    #[serde(default)]
    exec_fd: Option<RawFd>,
}
impl Executable {
    pub fn current() -> io::Result<Self> {
        Self::capture(&std::env::current_exe()?)
    }
    pub fn capture(path: &Path) -> io::Result<Self> {
        let image = open_executable(path)?;
        let metadata = image.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::other("binary is not a regular file"));
        }
        Ok(Self {
            path: path.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
            image: Some(std::sync::Arc::new(image)),
            #[cfg(target_os = "linux")]
            exec_fd: None,
        })
    }
    /// The held startup image when its path no longer names it and its
    /// content is unchanged. Unlinking the path updates the inode's change
    /// time, so content identity here is device, inode, size and mtime.
    fn replaced_image(&self) -> Option<&std::fs::File> {
        let image = self.image.as_deref()?;
        let held = image.metadata().ok()?;
        (self.verify().is_err()
            && (held.dev(), held.ino(), held.len()) == (self.device, self.inode, self.size)
            && (held.mtime(), held.mtime_nsec()) == self.modified)
            .then_some(image)
    }
    pub fn verify(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        let fresh = if let Some(fd) = self.exec_fd {
            image::fd_executable(&self.path, fd)?
        } else {
            Self::capture(&self.path)?
        };
        #[cfg(not(target_os = "linux"))]
        let fresh = Self::capture(&self.path)?;
        let changed: Vec<_> = [
            ("device", self.device != fresh.device),
            ("inode", self.inode != fresh.inode),
            ("size", self.size != fresh.size),
            ("mtime", self.modified != fresh.modified),
            ("ctime", self.changed != fresh.changed),
        ]
        .into_iter()
        .filter_map(|(field, differs)| differs.then_some(field))
        .collect();
        if !changed.is_empty() {
            return Err(io::Error::other(format!(
                "startup binary replaced ({} changed)",
                changed.join(", ")
            )));
        }
        Ok(())
    }
}
/// Descriptor-relative, no-follow open of a trusted binary (K13).
fn open_executable(path: &Path) -> io::Result<std::fs::File> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("binary parent missing"))?;
    let dir = DirFd::walk(parent, Trust::Operator).map_err(io::Error::other)?;
    let name = FileName::new(
        path.file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::other("invalid binary name"))?,
    )
    .map_err(io::Error::other)?;
    let (file, _) = dir
        .open_file(&name, libc::O_RDONLY, None, Trust::Operator)
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("binary missing"))?;
    Ok(file)
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
impl SentinelLease {
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
        .map_err(|e| RunError::Invalid(e.to_string()))?;
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
        .ok_or_else(|| RunError::Invalid("helper log missing".into()))?;
    root.dir().remove(&name, false).map_err(fs)?;
    Ok(file)
}
/// A helper that never started: nothing ran on its behalf.
fn helper_not_started(entry: &str, cause: io::Error) -> RunError {
    RunError::HelperExited {
        helper: entry.into(),
        exit: HelperExit::NotStarted,
        cause: cause.to_string(),
        stderr_tail: String::new(),
    }
}
/// Verifies the image and spawns `entry` with stderr to `log`. Refusal and
/// spawn failure are typed, never store I/O. An engine supervisor gets its
/// client's working directory and environment explicitly, never inherited.
/// Which admission authority it inherits from that client is explicit too:
/// only `SupervisorInheritance::ClientLease` forwards the parent capability.
fn spawn_helper(
    op: &OperationContext,
    executable: &Executable,
    entry: &str,
    log: &std::fs::File,
    launch: Option<&EngineLaunch>,
) -> Result<Child, RunError> {
    let mut busy = 0;
    loop {
        executable
            .verify()
            .map_err(|error| helper_not_started(entry, error))?;
        let mut command = helper(executable, entry).map_err(|e| helper_not_started(entry, e))?;
        if let Some(launch) = launch {
            command
                .envs(launch.env.iter().map(|(k, v)| (k, v)))
                .current_dir(&launch.cwd);
        }
        if entry == SENTINEL
            || launch.is_some_and(|launch| launch.inheritance == SupervisorInheritance::Independent)
        {
            // A registered sentinel owns fenced recovery, independent of an
            // ancestor's revocable admission lease. It never starts a workload.
            // An environment-owned supervisor is the same: it outlives the
            // `up` that started it, so it never keeps that process's lease.
            command.env_remove(crate::inheritance::ENV);
        } else if let Some(marker) = launch.and_then(|launch| {
            launch
                .env
                .iter()
                .find(|(key, _)| key == crate::inheritance::ENV)
                .map(|(_, value)| value)
        }) {
            let marker = marker.to_str().ok_or_else(|| {
                helper_not_started(
                    entry,
                    io::Error::other("invalid helper parent capability selector"),
                )
            })?;
            crate::inheritance::InheritedCapability::capture(op, marker)
                .and_then(|capability| capability.forward_command(op, &mut command))
                .map_err(|error| helper_not_started(entry, error))?;
        }
        command
            .stdout(Stdio::piped())
            .stderr(log.try_clone().map_err(|e| helper_not_started(entry, e))?);
        prepare_helper_exec(&mut command, executable, entry)
            .map_err(|error| helper_not_started(entry, error))?;
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
    RunError::HelperExited {
        helper: entry.into(),
        exit,
        cause: cause.to_string(),
        stderr_tail: helper_tail(log),
    }
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
    if let Some(address) = gates {
        let observe = cx
            .child(Duration::from_secs(10))
            .operation()
            .observe()
            .map_err(|_| RunError::Invalid("helper machine gate observation".into()))?;
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
/// Register a held-image exec only after all descriptor and environment
/// preparation. On Linux execveat never returns on success, so later callbacks
/// would otherwise be skipped.
fn prepare_helper_exec(
    command: &mut Command,
    executable: &Executable,
    entry: &str,
) -> io::Result<()> {
    #[cfg(target_os = "linux")]
    if let Some(fd) = executable.exec_fd {
        image::exec_held(command, executable, fd, entry)?;
    }
    #[cfg(not(target_os = "linux"))]
    let _ = (command, executable, entry);
    Ok(())
}

pub(crate) fn spawn_sentinel(
    run: &Run<'_>,
    root: &StateRoot,
    plan: &LifecyclePlan,
    next: &Invocation,
) -> Result<SentinelLease, RunError> {
    if plan.placement == Placement::Guest {
        return Err(RunError::HostAuthorityRequired);
    }
    let home = root
        .path()
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| RunError::Invalid("invalid state root".into()))?;
    let mut retried = false;
    loop {
        // A rebuilt path re-execs the startup image, never the replacement.
        let pin = private_image(root, &plan.executable)?;
        match sentinel_attempt(run, root, home, pin.as_ref(), plan, next) {
            // A rebuild can replace the path after `private_image` looked at it
            // (`cargo run` of Effigy beside a live run): this process's verify
            // refuses before spawn, or the sentinel's own refuses and it exits
            // unacknowledged. Either way nothing ran and nothing recorded it,
            // so one retry from the unchanged startup image is safe (I-SV4).
            Err(RunError::HelperExited { .. })
                if !retried && pin.is_none() && plan.executable.replaced_image().is_some() =>
            {
                retried = true;
            }
            // The sentinel verified and runs its image: the private image can go.
            result => return result,
        }
    }
}
fn sentinel_attempt(
    run: &Run<'_>,
    root: &StateRoot,
    home: &Path,
    pin: Option<&PrivateImage<'_>>,
    plan: &LifecyclePlan,
    next: &Invocation,
) -> Result<SentinelLease, RunError> {
    let executable = pin.map_or(&plan.executable, |pin| &pin.executable);
    let log = helper_log(root)?;
    // A detached owner runs in the environment its client handed over; its
    // sentinel settles the same effects, so it gets that environment too.
    // An inline owner's sentinel keeps a cleared environment. A sentinel never
    // inherits a parent capability, whichever placement spawns it.
    let inherited = (plan.placement == Placement::Detached).then(|| EngineLaunch {
        payload: Vec::new(),
        cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        env: std::env::vars_os().collect(),
        inheritance: SupervisorInheritance::Independent,
    });
    let mut child = spawn_helper(
        run.operation(),
        executable,
        SENTINEL,
        &log,
        inherited.as_ref(),
    )?;
    // The fault driver waits for every helper, registered or not (§12.1).
    #[cfg(all(test, feature = "fault-injection"))]
    crate::fault::note_helper(child.id());
    let mut writer = child
        .stdin
        .take()
        .ok_or_else(|| RunError::Invalid("sentinel pipe missing".into()))?;
    let config = SentinelConfig {
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
        .ok_or_else(|| RunError::Invalid("sentinel ready pipe missing".into()))?;
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
        .ok_or_else(|| RunError::Invalid("sentinel identity unavailable".into()))?;
    Ok(SentinelLease {
        identity,
        executable: plan.executable.clone(),
        child,
        writer: Some(writer),
    })
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
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > ResolvedInvocation::MAX_WIRE_BYTES {
        return Err(io::Error::other("helper frame too large"));
    }
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend(bytes);
    write_all(writer, &frame)
}
/// A bounded write to a nonblocking helper pipe.
fn write_all(writer: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    let op = context(CancelToken::default(), Duration::from_secs(30));
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
                emit(&Event::Refused(
                    "this binary runs no engine supervisor".into(),
                ));
                Err(RunError::Invalid("engine supervisor unsupported".into()))
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
            Work::Engine { payload } => {
                EngineSession::open(payload).and_then(|session| engine(session, router))
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
    let config: SupervisorConfig = serde_json::from_slice(&read_frame(&mut input)?)?;
    config
        .executable
        .verify()
        .map_err(|error| RunError::Invalid(format!("startup binary refused: {error}")))?;
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
    let config: SentinelConfig = serde_json::from_value(config)?;
    config
        .executable
        .verify()
        .map_err(|error| RunError::Invalid(format!("startup binary refused: {error}")))?;
    let next = Invocation::validated(
        config.next.into_iter().map(OsString::from_vec).collect(),
        validate_next,
    )
    .map_err(|_| RunError::Invalid("invalid recovery invocation".into()))?;
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
            Ok(_) => return Err(RunError::Invalid("unexpected sentinel pipe payload".into())),
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
        .map_err(|_| RunError::Invalid("sentinel context".into()))?;
    let root = helper_root(&cx, &config.home, config.machine, config.machine_gates)?;
    if root.is_machine() && config.owner.kind() == crate::id::ResourceKind::Gateway {
        let observe = op
            .observe()
            .map_err(|_| RunError::Invalid("sentinel observe".into()))?;
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
                .map_err(|_| RunError::Invalid("sentinel observe".into()))?,
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
            .map_err(|_| RunError::Invalid("sentinel observe".into()))?,
        &root,
        config.run,
    ) {
        if metadata.sentinel.is_none()
            && matches!(
                crate::gate::Gate::probe(
                    &op.observe()
                        .map_err(|_| RunError::Invalid("sentinel observe".into()))?,
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
        .map_err(|_| RunError::Invalid("cleanup overflow".into()))?;
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
    Refused(String),
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
            .map_err(|_| RunError::Invalid("cleanup overflow".into()))?;
        let cx = &cleanup
            .operation()
            .effects()
            .map_err(|_| RunError::Invalid("stop needs effects".into()))?;
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
        .map_err(|_| RunError::Invalid("detached spawn needs Mutate".into()))?;
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

/// The admission authority a detached supervisor inherits from the process
/// that starts it (§5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorInheritance {
    /// Keep the client's revocable parent capability. A managed run's
    /// supervisor runs as its client's owner: the client's death is that
    /// run's stop, so the lease is the same authority, and re-admitting the
    /// client's invocation is checked against it.
    ClientLease,
    /// Start with no parent capability at all. An environment-owned
    /// supervisor outlives the `up` that started it, so its store and journal
    /// opens must never depend on that process's revocable admission lease.
    /// Heavy work it starts itself falls back to its own admission hook.
    Independent,
}
/// What an engine's detached supervisor is started with besides its binary:
/// an opaque payload, the client's working directory and environment handed
/// over explicitly at spawn, and the admission authority it inherits from that
/// client. The engine admits its own run (§5.8).
#[derive(Clone)]
pub struct EngineLaunch {
    pub payload: Vec<u8>,
    pub cwd: PathBuf,
    pub env: Vec<(OsString, OsString)>,
    pub inheritance: SupervisorInheritance,
}
impl std::fmt::Debug for EngineLaunch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EngineLaunch { payload/cwd/env/inheritance: [REDACTED] }")
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
        .map_err(|_| RunError::Invalid("detached spawn needs Mutate".into()))?;
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
    };
    // Placeholder identity until the engine's `Admitted` names its run.
    let pending = (
        RunId::new(0, [0; 10]).map_err(|e| RunError::Invalid(e.to_string()))?,
        Fence {
            resource: ResourceId::store(crate::id::Scope::Machine, "engine-supervisor")
                .map_err(|e| RunError::Invalid(e.to_string()))?,
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
    // A revoked or unreadable parent capability is a store failure, not a
    // malformed request: name the admission step and the capability's cause.
    crate::inheritance::require_durable(cx.operation()).map_err(|error| RunError::StepFailed {
        step: "detached run admission".into(),
        cause: error.to_string(),
    })?;
    cx.require(crate::ctx::Capability::AdapterEffect)
        .map_err(|_| RunError::Invalid("detached spawn needs Mutate".into()))?;
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| RunError::HostAuthorityRequired)?;
    cx.operation().check("start detached")?;
    let observe = cx
        .child(Duration::from_secs(10))
        .operation()
        .observe()
        .map_err(|_| RunError::Invalid("detached publication context".into()))?;
    crate::migration::adoption::access(&observe, root)?;

    let config = SupervisorConfig {
        home: root.owning_directory().path().into(),
        machine: root.is_machine(),
        machine_gates: root.machine_gate_address(),
        executable: executable.clone(),
        work,
    };
    let log = helper_log(root)?;
    let mut child = spawn_helper(cx.operation(), executable, SUPERVISOR, &log, engine)?;
    let mut writer = child
        .stdin
        .take()
        .ok_or_else(|| RunError::Invalid("owner pipe missing".into()))?;
    let events = child
        .stdout
        .take()
        .ok_or_else(|| RunError::Invalid("event pipe missing".into()))?;
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
        Ok(Event::Refused(reason)) => {
            // The supervisor refused to admit its run and said why. That reason
            // is a store, gate or capability refusal, never an invalid request.
            return Err(RunError::StepFailed {
                step: "supervisor admission".into(),
                cause: reason,
            });
        }
        Ok(_) => return Err(RunError::Invalid("invalid admission event".into())),
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
        if eof && !self.detached {
            control.stop.get_or_insert(StopCause::OwnerLost);
        }
        while self.buffer.len() >= 4 {
            let size = u32::from_be_bytes(
                self.buffer[..4]
                    .try_into()
                    .map_err(|_| RunError::Invalid("client frame".into()))?,
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
    events: std::os::fd::OwnedFd,
    channel: OwningChannel,
    output: Option<OutputCapture>,
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
    fn open(payload: Vec<u8>) -> Result<Self, RunError> {
        let input = private_dup(libc::STDIN_FILENO)?;
        let events = private_dup(libc::STDOUT_FILENO)?;
        null_onto(libc::STDIN_FILENO)?;
        null_onto(libc::STDOUT_FILENO)?;
        Ok(Self {
            payload,
            events,
            channel: OwningChannel {
                input: std::fs::File::from(input),
                buffer: Vec::new(),
                listener: None,
                detached: false,
            },
            output: None,
        })
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
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
    /// Admission refused: the client reports `reason`; nothing ran. The
    /// reason also reaches the client's helper log tail, so a refusal too
    /// long for one event frame still names itself.
    pub fn refused(&self, reason: &str) {
        let _ = writeln!(io::stderr(), "{reason}");
        let mut end = reason.len().min(3072);
        while !reason.is_char_boundary(end) {
            end -= 1;
        }
        self.emit(&Event::Refused(reason[..end].into()));
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
        self.channel.turn(run, routed)
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
    pub fn capture_output(&mut self, run: &Run<'_>, secrets: Vec<Vec<u8>>) -> io::Result<()> {
        if self.output.is_none() {
            self.output = Some(OutputCapture::start(run, secrets)?);
        }
        Ok(())
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
}
/// The output log's journal position: no effect has sequence 0.
pub const OUTPUT_SEQUENCE: i64 = 0;
impl OutputCapture {
    fn start(run: &Run<'_>, secrets: Vec<Vec<u8>>) -> io::Result<Self> {
        let limits = run.limits();
        let mut logs = [
            crate::custody::Stream::Stdout,
            crate::custody::Stream::Stderr,
        ]
        .map(|stream| {
            run.capture_log(OUTPUT_SEQUENCE, stream, &limits)
                .ok()
                .flatten()
        });
        let mut readers = Vec::new();
        for target in [libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            let (read, write) = crate::custody::spawn::pipe()?;
            // SAFETY: dup2 of a live pipe end onto a standard descriptor.
            if unsafe { libc::dup2(write.as_raw_fd(), target) } < 0 {
                return Err(io::Error::last_os_error());
            }
            readers.push(read);
        }
        let thread = std::thread::Builder::new()
            .name("effigy-run-output".into())
            .spawn(move || {
                let mut redactor = crate::custody::Redactor::new(secrets);
                let mut open = [true, true];
                let mut buf = [0u8; 8192];
                while open.iter().any(|open| *open) {
                    let mut fds: Vec<libc::pollfd> = readers
                        .iter()
                        .map(|fd| libc::pollfd {
                            fd: fd.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        })
                        .collect();
                    // SAFETY: initialized pollfd array of its own length.
                    if unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 200) } < 0 {
                        continue;
                    }
                    for (index, fd) in fds.iter().enumerate() {
                        if !open[index] || fd.revents == 0 {
                            continue;
                        }
                        // SAFETY: live pipe read end and writable stack buffer.
                        let count =
                            unsafe { libc::read(fd.fd, buf.as_mut_ptr().cast(), buf.len()) };
                        if count <= 0 {
                            open[index] = false;
                            continue;
                        }
                        let tagged: Vec<(u8, u8)> = buf[..count as usize]
                            .iter()
                            .map(|b| (index as u8, *b))
                            .collect();
                        sink(&mut logs, redactor.push_tagged(&tagged, false));
                    }
                }
                sink(&mut logs, redactor.push_tagged(&[], true));
            })?;
        Ok(Self { thread })
    }
    /// Closes this process's write ends so the reader drains to EOF. A child
    /// still holding an inherited copy ends with its own settle.
    fn finish(self) {
        let _ = null_onto(libc::STDOUT_FILENO);
        let _ = null_onto(libc::STDERR_FILENO);
        let _ = self.thread.join();
    }
}
fn sink(logs: &mut [Option<crate::custody::capture::Log>; 2], clean: Vec<(u8, u8)>) {
    for (index, log) in logs.iter_mut().enumerate() {
        let part: Vec<u8> = clean
            .iter()
            .filter(|(tag, _)| usize::from(*tag) == index)
            .map(|(_, byte)| *byte)
            .collect();
        if part.is_empty() {
            continue;
        }
        // Observability only: a failed write marks the log, never the run (K10).
        if let Some(writer) = log.as_mut() {
            if writer.append(&part).is_err() {
                writer.mark_truncated();
                *log = None;
            }
        }
    }
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
        return Err(RunError::Invalid("process supervisor work".into()));
    };
    let inv =
        ResolvedInvocation::decode(&invocation).map_err(|e| RunError::Invalid(e.to_string()))?;
    let next = Invocation::validated(
        next.into_iter().map(OsString::from_vec).collect(),
        validate_next,
    )
    .map_err(|_| RunError::Invalid("invalid recovery invocation".into()))?;
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
        .map_err(|_| RunError::Invalid("supervisor context".into()))?;
    let root = helper_root(&cx, &home, machine, machine_gates)?;
    let cleanup = op
        .cleanup(Duration::from_secs(30))
        .map_err(|_| RunError::Invalid("cleanup overflow".into()))?;
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
            return Err(RunError::Invalid(
                "gateway must have one planned process".into(),
            ));
        };
        Run::begin_gateway(&cx, &root, &inv, options, &cleanup, adapter, process)
    } else {
        Run::begin(&cx, &root, &inv, options, &cleanup, adapter)
    };
    let mut run = match run {
        Ok(run) => run,
        Err(error) => {
            emit(&Event::Refused(error.to_string()));
            return Err(error);
        }
    };
    let listener = match run.control_socket() {
        Ok(listener) => listener,
        Err(error) => {
            emit(&Event::Refused(error.to_string()));
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
            emit(&Event::Refused(format!("{error:?}")));
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
                    .map_err(|_| RunError::Invalid("cleanup overflow".into()))?,
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
                        .map_err(|_| RunError::Invalid("cleanup overflow".into()))?,
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
            .map_err(|_| RunError::Invalid("cleanup overflow".into()))?,
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
