//! Pipe-only foreground sentinel. Receipts and inherited gates live in memory.
use super::*;
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
    output: ChildStdout,
    pub(crate) identity: Option<ProcessIdentity>,
    cx: EffectContext,
    close_deadline: Option<Instant>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    memory: bool,
    executable: Executable,
    owner: ProcessIdentity,
    gates: usize,
    run: RunId,
    fence: Fence,
}
#[derive(Serialize, Deserialize)]
enum Message {
    Register(Effect),
}

impl MemoryGuard {
    pub(crate) fn start(
        executable: &Executable,
        gate: &mut GateGuard,
        cx: &EffectContext,
        run: RunId,
        fence: Fence,
    ) -> Result<Self, RunError> {
        // Memory custody never pins or copies an image into runtime state.
        // A replaced image is a pre-go refusal, not permission to write one.
        cx.operation().check("foreground guard startup")?;
        executable.verify()?;
        #[cfg(target_os = "linux")]
        if executable.exec_fd.is_some() {
            return Err(RunError::Invalid(
                "foreground helper needs the verified startup path".into(),
            ));
        }
        let owner = ident::process_identity(std::process::id())
            .verified()
            .cloned()
            .ok_or_else(|| RunError::Invalid("foreground owner identity unavailable".into()))?;
        let copies = gate.share_custody()?;
        if copies.len() > GATE_MAX {
            return Err(RunError::Invalid("too many foreground gates".into()));
        }
        let config = Config {
            memory: true,
            executable: executable.clone(),
            owner,
            gates: copies.len(),
            run,
            fence,
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
                Ok(())
            });
        }
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("foreground guard input missing"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("foreground guard output missing"))?;
        let identity = ident::process_identity(child.id()).verified().cloned();
        // Install EOF ownership immediately: every later error drops the
        // writer and waits for this exact child, never an arbitrary pid.
        let mut guard = Self {
            child,
            input: Some(input),
            output,
            identity: identity.clone(),
            cx: cx.clone(),
            close_deadline: None,
        };
        nonblocking(
            guard
                .input
                .as_ref()
                .expect("input just installed")
                .as_raw_fd(),
        )?;
        nonblocking(guard.output.as_raw_fd())?;
        memory_frame(
            guard.input.as_mut().expect("input just installed"),
            &config,
            guard.cx.child(Duration::from_secs(30)).operation(),
        )?;
        guard.ack()?;
        if identity.is_none() {
            return Err(RunError::Invalid(
                "foreground guard identity unavailable".into(),
            ));
        }
        Ok(guard)
    }

    pub(crate) fn register(&mut self, effect: &Effect) -> Result<(), RunError> {
        if self.child.try_wait()?.is_some() {
            return Err(RunError::Invalid(
                "foreground guard exited before workload go".into(),
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
            match self.output.read(&mut byte) {
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
            return Err(RunError::Invalid(
                "invalid foreground guard acknowledgement".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn close(&mut self, cx: &CleanupContext) -> Result<(), RunError> {
        self.input.take();
        self.close_deadline = Some(Instant::now() + cx.operation().remaining().unwrap_or_default());
        loop {
            if let Some(exit) = self.child.try_wait()? {
                return if exit.success() {
                    Ok(())
                } else {
                    Err(RunError::Invalid("foreground guard left residue".into()))
                };
            }
            cx.operation().check("foreground guard closure")?;
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
impl Drop for MemoryGuard {
    fn drop(&mut self) {
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
        return Err(RunError::Invalid("foreground guard frame too large".into()));
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
    let config: Config = serde_json::from_value(config)?;
    config.executable.verify()?;
    if !config.memory
        || config.gates == 0
        || config.gates > GATE_MAX
        || ident::process_identity(config.owner.pid).verified() != Some(&config.owner)
    {
        return Err(RunError::Invalid(
            "invalid foreground guard owner or gates".into(),
        ));
    }
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
            return Err(RunError::Invalid(
                "untrusted inherited foreground gate".into(),
            ));
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
                        return Err(RunError::Invalid("foreground guard frame too large".into()));
                    }
                    let mut bytes = vec![0; size];
                    read_exact(input, &mut bytes)?;
                    let Message::Register(effect) = serde_json::from_slice(&bytes)?;
                    if effect.spec.kind != crate::custody::KIND
                        || effect.state != EffectState::Applied
                        || effect.sequence <= effects.last().map_or(0, |last| last.sequence)
                        || effect.spec.target != config.fence
                    {
                        return Err(RunError::Invalid(
                            "invalid foreground custody receipt".into(),
                        ));
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
        .map_err(|_| RunError::Invalid("foreground cleanup overflow".into()))?;
    let mut residue = false;
    for effect in effects.iter().rev() {
        // Exact identity and ready-anchor evidence only. Existing custody
        // settlement never treats a remembered PGID as signalling authority.
        let (state, _) = crate::custody::settle_effect(effect, None, &cleanup, || false);
        residue |= state != EffectState::Released;
    }
    drop(gates); // cleanup completed or residue is explicit; no numeric replay
    if residue {
        return Err(RunError::Invalid("foreground guard left residue".into()));
    }
    if let Some(error) = protocol_error {
        return Err(error);
    }
    Ok(())
}
