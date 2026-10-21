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
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::MetadataExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

mod image;
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
/// spawn failure are typed, never store I/O.
fn spawn_helper(
    executable: &Executable,
    entry: &str,
    log: &std::fs::File,
) -> Result<Child, RunError> {
    let mut busy = 0;
    loop {
        let mut command = helper(executable, entry).map_err(|e| helper_not_started(entry, e))?;
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
fn nonce() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos())
}
pub(crate) fn helper(executable: &Executable, entry: &str) -> io::Result<Command> {
    executable.verify()?;
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
    #[cfg(target_os = "linux")]
    if let Some(fd) = executable.exec_fd {
        image::exec_held(&mut command, executable, fd, entry)?;
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
    let mut child = spawn_helper(executable, SENTINEL, &log)?;
    // The fault driver waits for every helper, registered or not (§12.1).
    #[cfg(all(test, feature = "fault-injection"))]
    crate::fault::note_helper(child.id());
    let mut writer = child
        .stdin
        .take()
        .ok_or_else(|| RunError::Invalid("sentinel pipe missing".into()))?;
    let config = SentinelConfig {
        home: home.into(),
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
    let op = context(CancelToken::default(), Duration::from_secs(30));
    let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
    frame.extend(bytes);
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
pub fn internal_entry(
    entry: &str,
    router: &mut RouterHandle,
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
) -> Option<Result<(), RunError>> {
    let result = match entry {
        SENTINEL => sentinel(adapter, validate_next),
        SUPERVISOR => supervisor(router, adapter, validate_next),
        _ => return None,
    };
    // Before acknowledgement stderr is the parent's private log, so an early
    // failure names itself in the parent's typed error; afterwards it is null.
    if let Err(error) = &result {
        let _ = writeln!(io::stderr(), "{entry}: {error}");
    }
    Some(result)
}
const SENTINEL: &str = "__kernel-sentinel";
const SUPERVISOR: &str = "__kernel-supervise";
fn sentinel(
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
) -> Result<(), RunError> {
    let mut input = std::io::stdin().lock();
    nonblocking(input.as_raw_fd())?;
    let config: SentinelConfig = serde_json::from_slice(&read_frame(&mut input)?)?;
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
    let root = StateRoot::effects(&cx, &config.home).map_err(crate::store::StoreError::from)?;
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
    invocation: Vec<u8>,
    processes: Vec<WireProcess>,
    run: RunId,
    next: Vec<Vec<u8>>,
    executable: Executable,
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
}
#[derive(Debug)]
pub struct SupervisorClient {
    child: Child,
    writer: Option<ChildStdin>,
    events: std::process::ChildStdout,
    buffer: Vec<u8>,
    pub run: RunId,
    pub fence: Fence,
    home: PathBuf,
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
        let root = StateRoot::effects(cx, &self.home).map_err(crate::store::StoreError::from)?;
        run::request(
            cx,
            &root,
            &Request {
                id,
                run: self.run,
                expected: self.fence.clone(),
                action,
            },
        )?;
        if let Ok(mut socket) = UnixStream::connect(run::control_socket_path(
            &root,
            &self.fence.resource,
            self.run,
        )) {
            let _ = socket.write_all(&[1]);
        }
        Ok(())
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
        while !self.closed {
            self.event(cx)?;
        }
        loop {
            if self.child.try_wait()?.is_some() {
                return Ok(());
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
    cx.require(crate::ctx::Capability::HostAuthority)
        .map_err(|_| RunError::HostAuthorityRequired)?;
    cx.operation().check("start detached")?;
    let config = SupervisorConfig {
        home: home.into(),
        invocation: serde_json::to_vec(inv)?,
        processes: processes.into_iter().map(WireProcess::encode).collect(),
        run: id,
        next: next.argv().iter().map(|a| a.as_bytes().to_vec()).collect(),
        executable: executable.clone(),
    };
    let root = StateRoot::effects(cx, home).map_err(crate::store::StoreError::from)?;
    let log = helper_log(&root)?;
    let mut child = spawn_helper(executable, SUPERVISOR, &log)?;
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
        fence: Fence {
            resource: inv.spec().owner.clone(),
            generation: crate::id::Generation::new(0),
        },
        home: home.into(),
        active: false,
        closed: false,
    };
    match result.event(cx.operation()) {
        Ok(Event::Admitted { fence, .. }) => result.fence = fence,
        Ok(Event::Refused(reason)) => return Err(RunError::Invalid(reason)),
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
    if let Ok(bytes) = serde_json::to_vec(event) {
        let mut frame = (bytes.len() as u32).to_be_bytes().to_vec();
        frame.extend(bytes);
        if frame.len() <= 4096 {
            // SAFETY: immutable initialized frame and stdout fd. One nonblocking
            // write <= PIPE_BUF is atomic; backpressure never detaches a writer
            // or changes workload outcome. Redacted rotating logs retain output.
            unsafe { libc::write(libc::STDOUT_FILENO, frame.as_ptr().cast(), frame.len()) };
        }
    }
}

fn supervisor(
    router: &mut RouterHandle,
    adapter: &mut impl SettlementAdapter,
    validate_next: impl Fn(&[OsString]) -> bool,
) -> Result<(), RunError> {
    let mut input = std::io::stdin().lock();
    nonblocking(input.as_raw_fd())?;
    nonblocking(libc::STDOUT_FILENO)?;
    let config: SupervisorConfig = serde_json::from_slice(&read_frame(&mut input)?)?;
    config
        .executable
        .verify()
        .map_err(|error| RunError::Invalid(format!("startup binary refused: {error}")))?;
    let inv = ResolvedInvocation::decode(&config.invocation)
        .map_err(|e| RunError::Invalid(e.to_string()))?;
    let next = Invocation::validated(
        config.next.into_iter().map(OsString::from_vec).collect(),
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
    let root = StateRoot::effects(&cx, &config.home).map_err(crate::store::StoreError::from)?;
    let cleanup = op
        .cleanup(Duration::from_secs(30))
        .map_err(|_| RunError::Invalid("cleanup overflow".into()))?;
    let run = Run::begin(
        &cx,
        &root,
        &inv,
        BeginOpts {
            id: config.run,
            wait: Wait::FailFast,
            acknowledge: vec![],
            next,
            custody: Some(LifecyclePlan {
                executable: config.executable,
                placement: Placement::Detached,
            }),
        },
        &cleanup,
        adapter,
    );
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
    nonblocking(input.as_raw_fd())?;
    let specs = config
        .processes
        .into_iter()
        .map(WireProcess::decode)
        .collect::<io::Result<Vec<_>>>()?;
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
    let mut detached = false;
    let mut seen = vec![false; children.len()];
    'owner: loop {
        for event in router.poll() {
            for child in &mut children {
                child.forward(event.forward);
            }
        }
        // Observe EOF before reading durable requests: detach commits before
        // closing the client pipe. Reading requests first can miss a detach
        // committed between that read and EOF, spuriously stopping the run.
        let mut stop = op.cancel_token().cause().cloned();
        let mut owner_eof = false;
        if stop.is_none() {
            let (eof, requests) = observe_client_requests(&mut input, detached, || run.requests())?;
            owner_eof = eof;
            for request in requests {
                match request.action {
                    RequestAction::Detach => detached = true,
                    RequestAction::Escalate => {
                        for child in &mut children {
                            child.forward(libc::SIGKILL);
                        }
                        stop = Some(StopCause::Requested);
                    }
                    RequestAction::Stop | RequestAction::Settle => {
                        stop = Some(StopCause::Requested)
                    }
                }
                run.resolve_request(&request)?;
            }
        }
        loop {
            match listener.accept() {
                Ok((socket, _)) => {
                    if peer_owned(&socket) {
                        drop(socket);
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        if !detached && owner_eof {
            stop = Some(StopCause::OwnerLost);
        }
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
// EOF is causally after the client's last durable request. Keep the observation
// and request ordering together so detach cannot be mistaken for owner death.
fn observe_client_requests(
    input: &mut dyn Read,
    detached: bool,
    requests: impl FnOnce() -> Result<Vec<Request>, RunError>,
) -> Result<(bool, Vec<Request>), RunError> {
    let mut byte = [0u8; 1];
    let eof = !detached && matches!(input.read(&mut byte), Ok(0));
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
        let (eof, requests) = observe_client_requests(&mut Eof(&committed), false, || {
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
