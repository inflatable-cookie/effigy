//! Pipe-only foreground sentinel. Receipts and inherited gates live in memory.
use super::*;
use crate::closure::ClosurePin;
use crate::gate::GateGuard;
use crate::ident::{self, ProcessIdentity};
use crate::run::{Effect, EffectState};
use std::os::fd::{FromRawFd, OwnedFd};

use std::process::ChildStdout;

const GATE_FD: RawFd = 20;
const GATE_MAX: usize = 64;

#[derive(Debug)]
pub(crate) struct MemoryGuard {
    child: Child,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    pub(crate) identity: Option<ProcessIdentity>,
    cx: EffectContext,
    close_deadline: Option<Instant>,
    closure: Option<ClosurePin>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    protocol: u32,
    memory: bool,
    owner: ProcessIdentity,
    gates: usize,
    run: RunId,
    fence: Fence,
    watch: RawFd,
}
#[derive(Serialize, Deserialize)]
enum Message {
    Register(Effect),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryPurpose {
    Foreground,
    CheckoutLease,
}
impl MemoryGuard {
    pub(crate) fn start(
        executable: &Executable,
        gate: &mut GateGuard,
        ancestors: &[OwnedFd],
        cx: &EffectContext,
        run: RunId,
        fence: Fence,
        purpose: MemoryPurpose,
    ) -> Result<Self, RunError> {
        Self::start_recoverable(executable, gate, ancestors, cx, run, fence, purpose)
            .map_err(|(error, _guard)| error)
    }

    /// Preserve an already-started helper on startup failure. A returned
    /// partial guard is not qualified to register any process.
    pub(crate) fn start_recoverable(
        executable: &Executable,
        gate: &mut GateGuard,
        ancestors: &[OwnedFd],
        cx: &EffectContext,
        run: RunId,
        fence: Fence,
        purpose: MemoryPurpose,
    ) -> Result<Self, (RunError, Option<Box<Self>>)> {
        let (mut child, closure, config) = (|| -> Result<_, RunError> {
            // Memory custody never pins or copies an image into runtime state, and
            // never refuses over one: a rebuilt or missing binary beside a
            // foreground run is not a reason to stop it (K1).
            cx.operation()
                .check(if purpose == MemoryPurpose::Foreground {
                    "foreground guard startup"
                } else {
                    "checkout lease backup startup"
                })?;
            let owner = ident::process_identity(std::process::id())
                .verified()
                .cloned()
                .ok_or_else(|| RunError::unavailable("foreground owner identity unavailable"))?;
            let mut copies = gate.share_custody()?;
            for ancestor in ancestors {
                copies.push(crate::gate::duplicate_custody(ancestor)?);
            }
            if copies.len() > GATE_MAX {
                return Err(RunError::broken("too many foreground gates"));
            }
            let (watch, closure) = ClosurePin::new()?;
            let config = Config {
                protocol: HELPER_PROTOCOL,
                memory: true,
                owner,
                gates: copies.len(),
                run,
                fence,
                watch: watch.as_raw_fd(),
            };
            let mut command = helper(executable, SENTINEL)?;
            command.stdout(Stdio::piped());
            let raw: Vec<_> = copies.iter().map(AsRawFd::as_raw_fd).collect();
            // SAFETY: only async-signal-safe dup2 runs after fork. Sources are live
            // CLOEXEC duplicates >=128 owned by `copies` through spawn. Targets
            // 20..84 are distinct, never overlap sources or stdio. dup2 creates
            // the only inheritable gate fds, in this helper child only.
            unsafe {
                command.pre_exec(move || {
                    for (index, source) in raw.iter().enumerate() {
                        if libc::dup2(*source, GATE_FD + index as RawFd) < 0 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    // This live descriptor is already allocated >=128 in the
                    // parent. Clearing CLOEXEC cannot overwrite std's error pipe.
                    if libc::fcntl(watch.as_raw_fd(), libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = command.spawn()?;
            Ok((child, closure, config))
        })()
        .map_err(|error| (error, None))?;
        let input = child.stdin.take();
        let output = child.stdout.take();
        let identity = ident::process_identity(child.id()).verified().cloned();
        // Install EOF ownership immediately. Recoverable startup returns this
        // exact child and its pipes on any subsequent error.
        let mut guard = Self {
            child,
            input,
            output,
            identity: identity.clone(),
            cx: cx.clone(),
            close_deadline: None,
            closure: Some(closure),
        };
        let started = (|| -> Result<(), RunError> {
            if purpose == MemoryPurpose::CheckoutLease {
                crate::fault_point!(WorkspaceBackupAfterSpawn)?;
            }
            let input = guard
                .input
                .as_ref()
                .ok_or_else(|| io::Error::other("foreground guard input missing"))?;
            let output = guard
                .output
                .as_ref()
                .ok_or_else(|| io::Error::other("foreground guard output missing"))?;
            nonblocking(input.as_raw_fd())?;
            nonblocking(output.as_raw_fd())?;
            memory_frame(
                guard.input.as_mut().expect("input checked"),
                &config,
                guard.cx.child(Duration::from_secs(30)).operation(),
            )?;
            if purpose == MemoryPurpose::CheckoutLease {
                crate::fault_point!(WorkspaceBackupAfterConfig)?;
            }
            guard.ack()?;
            if purpose == MemoryPurpose::CheckoutLease {
                crate::fault_point!(WorkspaceBackupAfterAck)?;
            }
            if identity.is_none() {
                return Err(RunError::unavailable(
                    "foreground guard identity unavailable",
                ));
            }
            Ok(())
        })();
        match started {
            Ok(()) => Ok(guard),
            Err(error) => Err((error, Some(Box::new(guard)))),
        }
    }

    pub(crate) fn closure_pin(&self) -> Result<ClosurePin, RunError> {
        self.closure
            .as_ref()
            .cloned()
            .ok_or_else(|| RunError::unavailable("foreground custody pipe is closed"))
    }
    pub(crate) fn register(&mut self, effect: &Effect) -> Result<(), RunError> {
        if self.child.try_wait()?.is_some() {
            return Err(RunError::unavailable(
                "foreground guard exited before workload go",
            ));
        }
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("foreground guard closed"))?;
        memory_frame(
            input,
            &Message::Register(effect.clone()),
            self.cx.child(Duration::from_secs(30)).operation(),
        )?;
        self.ack()
    }
    fn ack(&mut self) -> Result<(), RunError> {
        let mut byte = [0];
        let cx = self.cx.child(Duration::from_secs(30));
        loop {
            cx.operation().check("foreground guard acknowledgement")?;
            let output = self
                .output
                .as_mut()
                .ok_or_else(|| io::Error::other("foreground guard output missing"))?;
            match output.read(&mut byte) {
                Ok(1) => break,
                Ok(_) => {
                    return Err(
                        io::Error::other("foreground guard closed acknowledgement pipe").into(),
                    )
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                    ) =>
                {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => return Err(error.into()),
            }
        }
        if byte != [1] {
            return Err(RunError::broken("invalid foreground guard acknowledgement"));
        }
        Ok(())
    }
    pub(crate) fn close(&mut self, cx: &CleanupContext) -> Result<(), RunError> {
        self.closure.take();
        self.input.take();
        self.close_deadline = Some(Instant::now() + cx.operation().remaining().unwrap_or_default());
        loop {
            if let Some(exit) = self.child.try_wait()? {
                return if exit.success() {
                    Ok(())
                } else {
                    Err(RunError::broken("foreground guard left residue"))
                };
            }
            cx.operation().check("foreground guard closure")?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for MemoryGuard {
    fn drop(&mut self) {
        self.closure.take();
        self.input.take();
        // EOF starts bounded guard cleanup. Do not kill a custodian or unlock
        // its gates just because the caller's cleanup budget has expired.
        let deadline = self
            .close_deadline
            .unwrap_or_else(|| Instant::now() + Duration::from_secs(60));
        while matches!(self.child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

fn memory_frame(
    writer: &mut impl Write,
    value: &impl Serialize,
    cx: &OperationContext,
) -> Result<(), RunError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > ResolvedInvocation::MAX_WIRE_BYTES {
        return Err(RunError::broken("foreground guard frame too large"));
    }
    let mut frame = Vec::with_capacity(bytes.len() + 4);
    frame.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    frame.extend_from_slice(&bytes);
    let mut remaining = frame.as_slice();
    while !remaining.is_empty() {
        cx.check("foreground guard write")?;
        match writer.write(remaining) {
            Ok(0) => return Err(io::Error::other("foreground guard pipe closed").into()),
            Ok(n) => remaining = &remaining[n..],
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn serve(config: serde_json::Value, input: &mut impl Read) -> Result<(), RunError> {
    // The version before the shape: an update that changed this frame still
    // reads as a version mismatch, not as a broken store value (F8).
    let declared = config
        .get("protocol")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| {
            RunError::refused("this helper frame declares no protocol, so rerun the command")
        })?;
    super::check_protocol(u32::try_from(declared).unwrap_or(u32::MAX))?;
    let config: Config = serde_json::from_value(config)?;
    if !config.memory
        || config.gates == 0
        || config.gates > GATE_MAX
        || ident::process_identity(config.owner.pid).verified() != Some(&config.owner)
    {
        return Err(RunError::broken("invalid foreground guard owner or gates"));
    }
    if config.watch < 128 {
        return Err(RunError::broken("invalid foreground closure descriptors"));
    }
    let mut watch = inherited_pipe(config.watch, libc::O_RDONLY)?;
    let mut gates: Vec<OwnedFd> = Vec::new();
    for index in 0..config.gates {
        let raw = GATE_FD + index as RawFd;
        // fstat validates each inherited fd before it is adopted. These fds
        // are private regular gate files, never sockets or workload pipes.
        let stat = crate::fs::fstat(raw)?;
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG
            || stat.st_uid != crate::fs::operator_uid()
            || stat.st_mode & 0o077 != 0
        {
            return Err(RunError::broken("untrusted inherited foreground gate"));
        }
        // SAFETY: the parent pre-exec created these distinct inherited fds.
        // fstat verified each exists; no other owner is constructed for them.
        gates.push(unsafe { OwnedFd::from_raw_fd(raw) });
        // SAFETY: this helper owns raw; ensure its future forks cannot leak it.
        if unsafe { libc::fcntl(raw, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error().into());
        }
    }
    let mut output = std::io::stdout().lock();
    output.write_all(&[1])?;
    output.flush()?;
    let mut effects: Vec<Effect> = Vec::new();
    // Registration frames are length-prefixed. Reading the first byte is the
    // only intentionally open wait: owner EOF triggers cleanup, never unlock.
    let mut protocol_error = None;
    loop {
        let mut first = [0];
        match input.read(&mut first) {
            Ok(0) => break,
            Ok(_) => {
                let result = (|| -> Result<(), RunError> {
                    let mut size = [0; 4];
                    size[0] = first[0];
                    read_exact(input, &mut size[1..])?;
                    let size = u32::from_be_bytes(size) as usize;
                    if size > ResolvedInvocation::MAX_WIRE_BYTES {
                        return Err(RunError::broken("foreground guard frame too large"));
                    }
                    let mut bytes = vec![0; size];
                    read_exact(input, &mut bytes)?;
                    let Message::Register(effect) = serde_json::from_slice(&bytes)?;
                    if effect.spec.kind != crate::custody::KIND
                        || effect.state != EffectState::Applied
                        || effect.sequence <= effects.last().map_or(0, |last| last.sequence)
                        || effect.spec.target != config.fence
                    {
                        return Err(RunError::broken("invalid foreground custody receipt"));
                    }
                    crate::custody::verify_foreground_receipt(&effect, config.run)?;
                    // Acknowledge only after a complete receipt is retained.
                    effects.push(effect);
                    output.write_all(&[1])?;
                    output.flush()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    protocol_error = Some(error);
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                ) =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => {
                protocol_error = Some(error.into());
                break;
            }
        }
    }
    drop(output);
    let cleanup = context(CancelToken::default(), Duration::from_secs(60))
        .cleanup(Duration::from_secs(60))
        .map_err(|_| RunError::broken("foreground cleanup overflow"))?;
    let mut residue = false;
    for effect in effects.iter().rev() {
        // Exact identity and ready-anchor evidence only. Existing custody
        // settlement never treats a remembered PGID as signalling authority.
        let (state, _) = crate::custody::settle_effect(effect, None, &cleanup, || false);
        residue |= state != EffectState::Released;
    }
    // Each delegated guard retains a pipe writer until its own cleanup completes.
    // An absent/stopped owner is never permission to release an ancestor early.
    // Like the owner EOF above, this is a liveness wait, not a fresh effect or
    // numeric replay. Unknown closure keeps these checkout gates held.
    let mut bytes = [0u8; 4096];
    loop {
        match watch.read(&mut bytes) {
            Ok(0) => break,
            Ok(_) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    }
    drop(gates); // positive delegated closure; no numeric replay
    if residue {
        return Err(RunError::broken("foreground guard left residue"));
    }
    if let Some(error) = protocol_error {
        return Err(error);
    }
    Ok(())
}

fn inherited_pipe(fd: RawFd, access: i32) -> io::Result<std::fs::File> {
    // Reject malformed descriptors before adopting ownership. In particular,
    // an invalid frame cannot close one of this helper's regular image files.
    let stat = crate::fs::fstat(fd)?;
    if stat.st_mode & libc::S_IFMT != libc::S_IFIFO || stat.st_uid != crate::fs::operator_uid() {
        return Err(io::Error::other("untrusted foreground custody pipe"));
    }
    // SAFETY: read-only fcntl on the descriptor just validated by fstat;
    // it accepts no pointer and a closed descriptor returns EBADF.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::O_ACCMODE != access {
        return Err(io::Error::other(
            "foreground custody pipe has the wrong access mode",
        ));
    }
    // SAFETY: the private startup frame names a distinct inherited pipe fd
    // allocated >=128 by the parent. It cannot alias gate/stdio slots and
    // no other owner has been constructed for it in this helper.
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let file = std::fs::File::from(owned);
    // SAFETY: file retains this live descriptor across fcntl; only its own
    // descriptor flag changes, so future execs cannot inherit the pin.
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(io::Error::last_os_error());
    }
    if access == libc::O_RDONLY {
        nonblocking(fd)?;
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_closure_descriptor_never_adopts_a_closed_or_regular_handle() {
        assert!(inherited_pipe(1_000_000, libc::O_RDONLY).is_err());
        let fixture = tempfile::tempdir().expect("private fixture");
        let file = std::fs::File::create(fixture.path().join("image")).expect("regular handle");
        let duplicate = crate::gate::duplicate_custody(&file).expect("private duplicate");
        assert!(inherited_pipe(duplicate.as_raw_fd(), libc::O_WRONLY).is_err());
        crate::fs::fstat(duplicate.as_raw_fd()).expect("refusal did not consume the handle");
    }
}
