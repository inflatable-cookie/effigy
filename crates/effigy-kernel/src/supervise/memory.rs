//! Pipe-only foreground sentinel. Receipts and inherited gates live in memory.
use super::*;
use crate::closure::ClosurePin;
use crate::gate::GateGuard;
use crate::ident::{self, ProcessIdentity};
use crate::run::{Effect, EffectState};
use std::os::fd::{FromRawFd, OwnedFd};

use std::process::ChildStdout;

const GATE_MAX: usize = 64;

#[derive(Debug)]
pub(crate) struct MemoryGuard {
    child: Child,
    descriptors: Option<std::os::unix::net::UnixDatagram>,
    has_interpolation: bool,
    unconfirmed_files: std::collections::BTreeSet<String>,
    confirmed_files: std::collections::BTreeSet<String>,
    pending_files: std::collections::BTreeMap<String, u64>,
    pending_finish: Option<u64>,
    input: Option<ChildStdin>,
    output: Option<ChildStdout>,
    pub(crate) identity: Option<ProcessIdentity>,
    cx: EffectContext,
    close_deadline: Option<Instant>,
    closure: Option<ClosurePin>,
    /// Monotonic per-guard request number: a stale acknowledgement carries the
    /// previous number and never answers the frame just written. A number is
    /// reserved before its frame is written and is never reused, even when the
    /// write or its acknowledgement fails.
    requests: Requests,
    /// How long one frame waits for its own acknowledgement, within the
    /// caller's remaining context budget.
    ack_budget: Duration,
}

/// Owns a helper whose startup did not complete. Deliberately exposes no
/// process-registration operation; only bounded cleanup is available.
#[derive(Debug)]
pub(crate) struct PartialMemoryGuard(MemoryGuard);

impl PartialMemoryGuard {
    pub(crate) fn close(&mut self, cx: &CleanupContext) -> Result<(), RunError> {
        self.0.close(cx)
    }

    #[cfg(all(test, feature = "fault-injection"))]
    pub(crate) fn identity(&self) -> Option<ProcessIdentity> {
        self.0.identity.clone()
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Config {
    pub(super) protocol: u32,
    pub(super) memory: bool,
    pub(super) owner: ProcessIdentity,
    pub(super) gates: usize,
    pub(super) gate_fds: Vec<RawFd>,
    pub(super) run: RunId,
    pub(super) fence: Fence,
    pub(super) watch: RawFd,
    #[serde(default)]
    pub(super) descriptors: Option<RawFd>,
}
#[derive(Serialize, Deserialize)]
enum Message {
    /// A receipt frame names the request it answers, so its acknowledgement
    /// cannot be confused with a late acknowledgement of an earlier frame.
    Register {
        effect: Effect,
        request: u64,
    },
    Interpolation {
        binding: crate::interpolation::Binding,
        request: u64,
    },
    /// Hold-only SCM_RIGHTS custody. This is never process or cleanup authority.
    Prelaunch {
        sequence: i64,
        original: String,
        location: crate::interpolation::Location,
        request: u64,
    },
    ReleasePrelaunch {
        sequence: i64,
        request: u64,
    },
    Finish {
        request: u64,
    },
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
    ) -> Result<Self, (RunError, Option<Box<PartialMemoryGuard>>)> {
        let (mut child, closure, config, descriptors) = (|| -> Result<_, RunError> {
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
            let (descriptors, receiver) = std::os::unix::net::UnixDatagram::pair()?;
            let receiver = crate::gate::duplicate_custody(&receiver)?;
            descriptors.set_write_timeout(Some(ACK_BUDGET))?;
            let config = Config {
                protocol: HELPER_PROTOCOL,
                memory: true,
                owner,
                gates: copies.len(),
                gate_fds: copies.iter().map(AsRawFd::as_raw_fd).collect(),
                run,
                fence,
                watch: watch.as_raw_fd(),
                descriptors: Some(receiver.as_raw_fd()),
            };
            let mut command = helper(executable, SENTINEL)?;
            command.stdout(Stdio::piped());
            let raw: Vec<_> = copies.iter().map(AsRawFd::as_raw_fd).collect();
            // SAFETY: only async-signal-safe fcntl runs after fork. Each fd is
            // already allocated and held by `copies` through spawn, so it cannot
            // collide with a later stdio/error pipe. Only this helper child
            // clears CLOEXEC; the owner's copies stay private to the owner.
            unsafe {
                command.pre_exec(move || {
                    for source in &raw {
                        if libc::fcntl(*source, libc::F_SETFD, 0) < 0 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    // This live descriptor is already allocated >=128 in the
                    // parent. Clearing CLOEXEC cannot overwrite std's error pipe.
                    if libc::fcntl(watch.as_raw_fd(), libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if libc::fcntl(receiver.as_raw_fd(), libc::F_SETFD, 0) < 0 {
                        return Err(io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let child = command.spawn()?;
            Ok((child, closure, config, descriptors))
        })()
        .map_err(|error| (error, None))?;
        #[cfg(all(test, feature = "fault-injection"))]
        crate::fault::note_backup(child.id());
        let input = child.stdin.take();
        let output = child.stdout.take();
        let identity = ident::process_identity(child.id()).verified().cloned();
        // Install EOF ownership immediately. Recoverable startup returns this
        // exact child and its pipes on any subsequent error.
        let mut guard = Self {
            child,
            descriptors: Some(descriptors),
            has_interpolation: false,
            unconfirmed_files: Default::default(),
            confirmed_files: Default::default(),
            pending_files: Default::default(),
            pending_finish: None,
            input,
            output,
            identity: identity.clone(),
            cx: cx.clone(),
            close_deadline: None,
            closure: Some(closure),
            requests: Requests::default(),
            ack_budget: ACK_BUDGET,
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
            guard.ready()?;
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
            Err(error) => Err((error, Some(Box::new(PartialMemoryGuard(guard))))),
        }
    }

    /// A guard around a helper this test spawned itself, for driving the
    /// owner-side frame and acknowledgement path without a real custodian. It
    /// grants no custody: the recorded identity is only that process's own.
    #[cfg(all(test, feature = "fault-injection"))]
    pub(super) fn testing(mut child: Child, cx: EffectContext, ack_budget: Duration) -> Self {
        let input = child.stdin.take().expect("piped guard stdin");
        let output = child.stdout.take().expect("piped guard stdout");
        // The owner always writes and reads these pipes nonblocking, so its
        // deadline check is reached between polls. Keep the fixture identical.
        nonblocking(input.as_raw_fd()).expect("nonblocking guard stdin");
        nonblocking(output.as_raw_fd()).expect("nonblocking guard stdout");
        let identity = ident::process_identity(child.id()).verified().cloned();
        Self {
            child,
            descriptors: None,
            has_interpolation: false,
            unconfirmed_files: Default::default(),
            confirmed_files: Default::default(),
            pending_files: Default::default(),
            pending_finish: None,
            input: Some(input),
            output: Some(output),
            identity,
            cx,
            close_deadline: None,
            closure: None,
            requests: Requests::default(),
            ack_budget,
        }
    }

    #[cfg(all(test, feature = "fault-injection"))]
    pub(super) fn testing_child(&mut self) -> &mut Child {
        &mut self.child
    }
    pub(crate) fn closure_pin(&self) -> Result<ClosurePin, RunError> {
        self.closure
            .as_ref()
            .cloned()
            .ok_or_else(|| RunError::unavailable("foreground custody pipe is closed"))
    }
    pub(crate) fn register(&mut self, effect: &Effect) -> Result<(), RunError> {
        // The retained backup is the one this process started: re-prove its
        // exact incarnation before a frame is written to it. A guard that has
        // exited or been replaced gets no receipt, and stays retained.
        self.verify_retained()?;
        // Reserve the number BEFORE the write. A frame that may have reached
        // the guard burns its number even when the acknowledgement is lost or
        // late, so the next frame can never reuse it and a late answer can
        // never satisfy a later receipt.
        let request = self.requests.reserve()?;
        let input = self
            .input
            .as_mut()
            .ok_or_else(|| io::Error::other("foreground guard closed"))?;
        memory_frame(
            input,
            &Message::Register {
                effect: effect.clone(),
                request,
            },
            self.cx.child(Duration::from_secs(30)).operation(),
        )?;
        self.ack(request)
    }
    pub(crate) fn owns_interpolation(
        &self,
        file: &crate::interpolation::RetainedInterpolation,
    ) -> bool {
        file.binding().is_ok_and(|binding| {
            self.confirmed_files.contains(binding.invocation())
                && !self.unconfirmed_files.contains(binding.invocation())
        })
    }
    pub(crate) fn has_unconfirmed_files(&self) -> bool {
        !self.unconfirmed_files.is_empty()
    }
    pub(crate) fn has_interpolation(&self) -> bool {
        self.has_interpolation
    }
    pub(crate) fn retain_prelaunch(
        &mut self,
        sequence: i64,
        file: &crate::interpolation::RetainedInterpolation,
    ) -> Result<(), RunError> {
        self.verify_retained()?;
        let request = self.requests.reserve()?;
        self.has_interpolation = true;
        let socket = self
            .descriptors
            .as_ref()
            .ok_or_else(|| RunError::refused("helper has no descriptor custody channel"))?;
        crate::interpolation::channel::send(socket, request, &file.prelaunch_descriptors()?)?;
        let (original, location) = file.prelaunch_name();
        memory_frame(
            self.input
                .as_mut()
                .ok_or_else(|| RunError::unavailable("custody input closed"))?,
            &Message::Prelaunch {
                sequence,
                original: original.into(),
                location: location.clone(),
                request,
            },
            self.cx.child(ACK_BUDGET).operation(),
        )?;
        self.ack(request)
    }
    pub(crate) fn release_prelaunch(&mut self, sequence: i64) -> Result<(), RunError> {
        self.verify_retained()?;
        let request = self.requests.reserve()?;
        memory_frame(
            self.input
                .as_mut()
                .ok_or_else(|| RunError::unavailable("custody input closed"))?,
            &Message::ReleasePrelaunch { sequence, request },
            self.cx.child(ACK_BUDGET).operation(),
        )?;
        self.ack(request)
    }
    pub(crate) fn retain_interpolation(
        &mut self,
        file: &crate::interpolation::RetainedInterpolation,
    ) -> Result<(), RunError> {
        self.verify_retained()?;
        let invocation = file.binding()?.invocation().to_owned();
        self.unconfirmed_files.insert(invocation.clone());
        let request = match self.pending_files.get(&invocation) {
            Some(request) => *request,
            None => {
                let request = self.requests.reserve()?;
                self.pending_files.insert(invocation.clone(), request);
                request
            }
        };
        let socket = self
            .descriptors
            .as_ref()
            .ok_or_else(|| RunError::refused("helper has no descriptor custody channel"))?;
        self.has_interpolation = true;
        crate::interpolation::channel::send(socket, request, &file.descriptors()?)?;
        memory_frame(
            self.input
                .as_mut()
                .ok_or_else(|| RunError::unavailable("custody input closed"))?,
            &Message::Interpolation {
                binding: file.binding()?.clone(),
                request,
            },
            self.cx.child(ACK_BUDGET).operation(),
        )?;
        self.ack(request)?;
        self.pending_files.remove(&invocation);
        self.unconfirmed_files.remove(&invocation);
        self.confirmed_files.insert(invocation);
        crate::fault_point!(InterpolationRegistered)?;
        Ok(())
    }
    /// Read-only proof that the retained guard is still the exact process this
    /// owner started and that its pipe is still ours.
    pub(crate) fn verify_retained(&mut self) -> Result<(), RunError> {
        if self.child.try_wait()?.is_some() {
            return Err(RunError::unavailable(
                "foreground guard exited before workload go",
            ));
        }
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| RunError::unavailable("foreground guard identity unavailable"))?;
        if ident::process_identity(identity.pid).verified() != Some(identity)
            || !matches!(
                ident::process_state(identity.pid),
                Observed::Verified(
                    ident::ProcessState::Active | ident::ProcessState::Stopped,
                    _
                )
            )
            || self.input.is_none()
        {
            return Err(RunError::unavailable(
                "foreground guard identity unavailable",
            ));
        }
        Ok(())
    }
    /// The one-byte startup readiness answer: the guard has adopted its config
    /// and gates. It is not a receipt registration.
    fn ready(&mut self) -> Result<(), RunError> {
        let mut byte = [0];
        let cx = self.cx.child(self.ack_budget);
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
    fn ack(&mut self, request: u64) -> Result<(), RunError> {
        // A mismatch is a refusal, never a retry: the guard may already hold
        // the frame this answer did not name, so the owner keeps it as
        // unsettled custody rather than replaying a receipt.
        let cx = self.cx.child(self.ack_budget);
        let output = self
            .output
            .as_mut()
            .ok_or_else(|| io::Error::other("foreground guard output missing"))?;
        read_acknowledgement(output, request, cx.operation())
    }
    pub(crate) fn close(&mut self, cx: &CleanupContext) -> Result<(), RunError> {
        if self.has_unconfirmed_files() {
            return Err(RunError::unavailable(
                "interpolation handoff remains unconfirmed",
            ));
        }
        if self.has_interpolation && self.input.is_some() {
            self.verify_retained()?;
            let request = match self.pending_finish {
                Some(request) => request,
                None => {
                    let request = self.requests.reserve()?;
                    self.pending_finish = Some(request);
                    request
                }
            };
            memory_frame(
                self.input.as_mut().expect("input checked"),
                &Message::Finish { request },
                cx.operation(),
            )?;
            let output = self
                .output
                .as_mut()
                .ok_or_else(|| RunError::unavailable("custody output closed"))?;
            read_acknowledgement(output, request, cx.operation())?;
            let mut answer = [0];
            loop {
                cx.operation().check("interpolation cleanup answer")?;
                match output.read(&mut answer) {
                    Ok(1) => break,
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                        ) =>
                    {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    _ => {
                        return Err(RunError::unavailable(
                            "interpolation cleanup answer unavailable",
                        ))
                    }
                }
            }
            crate::fault_point!(InterpolationFinishReply)?;
            self.pending_finish = None;
            if answer != [1] {
                return Err(RunError::unavailable(
                    "retained interpolation cleanup remains Unknown",
                ));
            }
        }
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

/// How long one receipt frame waits for its own acknowledgement.
const ACK_BUDGET: Duration = Duration::from_secs(30);

/// The per-guard receipt request numbers. A number is reserved before its frame
/// is written, so a frame that may have reached the guard — or whose answer was
/// lost — can never share its number with a later frame.
#[derive(Debug, Default)]
pub(super) struct Requests {
    last: u64,
}
impl Requests {
    pub(super) fn reserve(&mut self) -> Result<u64, RunError> {
        self.last = self
            .last
            .checked_add(1)
            .ok_or_else(|| RunError::refused("foreground custody request numbers exhausted"))?;
        Ok(self.last)
    }
}

/// Read the one answer a frame may accept: eight big-endian bytes naming that
/// exact request. Any other value is a stale or foreign answer and is refused;
/// it must never stand in for the acknowledgement of the frame just written.
fn read_acknowledgement(
    reader: &mut impl Read,
    request: u64,
    cx: &crate::ctx::OperationContext,
) -> Result<(), RunError> {
    let mut bytes = [0u8; 8];
    let mut offset = 0;
    while offset < bytes.len() {
        cx.check("foreground guard acknowledgement")?;
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => {
                return Err(io::Error::other("foreground guard closed acknowledgement pipe").into())
            }
            Ok(n) => offset += n,
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
    if u64::from_be_bytes(bytes) != request {
        return Err(RunError::broken(
            "foreground guard acknowledged another request",
        ));
    }
    Ok(())
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
    let mut inherited = config.gate_fds.clone();
    inherited.push(config.watch);
    if let Some(descriptors) = config.descriptors {
        inherited.push(descriptors);
    }
    inherited.sort_unstable();
    if config.gate_fds.len() != config.gates
        || inherited.iter().any(|fd| *fd < 128)
        || inherited.windows(2).any(|pair| pair[0] == pair[1])
    {
        return Err(RunError::broken(
            "invalid or aliased foreground custody descriptors",
        ));
    }
    let descriptors = match config.descriptors {
        Some(fd) if fd >= 128 && fd != config.watch => {
            let stat = crate::fs::fstat(fd)?;
            if stat.st_mode & libc::S_IFMT != libc::S_IFSOCK
                || stat.st_uid != crate::fs::operator_uid()
            {
                return Err(RunError::refused("untrusted descriptor custody socket"));
            }
            // SAFETY: startup created this distinct inherited socket descriptor;
            // it is outside stdio/gate slots and has no other Rust owner here.
            let socket = unsafe { std::os::unix::net::UnixDatagram::from_raw_fd(fd) };
            nix::fcntl::fcntl(
                &socket,
                nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC),
            )
            .map_err(io::Error::from)?;
            socket.set_read_timeout(Some(ACK_BUDGET))?;
            Some(socket)
        }
        None => None,
        _ => return Err(RunError::refused("invalid descriptor custody socket")),
    };
    let mut watch = inherited_pipe(config.watch, libc::O_RDONLY)?;
    let mut gates: Vec<OwnedFd> = Vec::new();
    for &raw in &config.gate_fds {
        // fstat validates each inherited fd before it is adopted. These fds
        // are private regular gate files, never sockets or workload pipes.
        let stat = crate::fs::fstat(raw)?;
        if stat.st_mode & libc::S_IFMT != libc::S_IFREG
            || stat.st_uid != crate::fs::operator_uid()
            || stat.st_mode & 0o077 != 0
        {
            return Err(RunError::broken("untrusted inherited foreground gate"));
        }
        // SAFETY: startup lists distinct preallocated inherited fds. Their
        // ranges/aliases and fstat were checked before adoption; no other
        // Rust owner exists for them in this helper.
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
    let mut files: Vec<crate::interpolation::RetainedInterpolation> = Vec::new();
    let mut prelaunch_files: std::collections::BTreeMap<
        i64,
        crate::interpolation::RetainedInterpolation,
    > = Default::default();
    let mut last_request = 0u64;
    let mut finishing = false;
    let mut retained_completed = false;
    let mut last_file: Option<(u64, String)> = None;
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
                    let request = match serde_json::from_slice(&bytes)? {
                        Message::Finish { request } => {
                            if request < last_request || request == 0 {
                                return Err(RunError::refused("stale retained cleanup request"));
                            }
                            finishing = true;
                            let complete = prelaunch_files.is_empty()
                                && (retained_completed
                                    || finish_interpolations(&effects, &mut files, &config)?);
                            retained_completed |= complete;
                            last_request = request;
                            output.write_all(&request.to_be_bytes())?;
                            output.write_all(&[u8::from(complete)])?;
                            output.flush()?;
                            return Ok(());
                        }
                        Message::Prelaunch {
                            sequence,
                            original,
                            location,
                            request,
                        } => {
                            if finishing || request <= last_request || sequence <= 0 {
                                return Err(RunError::refused("invalid prelaunch custody request"));
                            }
                            let socket = descriptors
                                .as_ref()
                                .ok_or_else(|| RunError::refused("no descriptor custody socket"))?;
                            let file =
                                crate::interpolation::RetainedInterpolation::receive_prelaunch(
                                    original,
                                    location,
                                    crate::interpolation::channel::receive(socket, request)?,
                                )?;
                            if let Some(retained) = prelaunch_files.get(&sequence) {
                                if !retained.same_object(&file) {
                                    return Err(RunError::refused(
                                        "prelaunch retry changed its owner",
                                    ));
                                }
                            } else {
                                if prelaunch_files.len() >= 64 {
                                    return Err(RunError::refused("too many prelaunch owners"));
                                }
                                prelaunch_files.insert(sequence, file);
                            }
                            request
                        }
                        Message::ReleasePrelaunch { sequence, request } => {
                            if finishing || request <= last_request {
                                return Err(RunError::refused("invalid prelaunch release request"));
                            }
                            if let Some(file) = prelaunch_files.get(&sequence) {
                                if !file.prelaunch_unlinked()? {
                                    return Err(RunError::refused(
                                        "prelaunch file is not positively unlinked",
                                    ));
                                }
                                prelaunch_files.remove(&sequence);
                            }
                            request
                        }
                        Message::Register { effect, request } => {
                            if finishing {
                                return Err(RunError::refused("custody is already closing"));
                            }
                            validate_register(
                                &effect,
                                request,
                                last_request,
                                effects.last().map_or(0, |last| last.sequence),
                                &config,
                            )?;
                            effects.push(effect);
                            request
                        }
                        Message::Interpolation { binding, request } => {
                            if finishing {
                                return Err(RunError::refused("custody is already closing"));
                            }
                            if request < last_request
                                || (request == last_request
                                    && last_file.as_ref()
                                        != Some(&(request, binding.invocation().to_owned())))
                            {
                                return Err(RunError::refused("stale interpolation request"));
                            }
                            let invocation = binding.invocation().to_owned();
                            let socket = descriptors
                                .as_ref()
                                .ok_or_else(|| RunError::refused("no descriptor custody socket"))?;
                            let received = crate::interpolation::channel::receive(socket, request)?;
                            let file = crate::interpolation::RetainedInterpolation::receive(
                                binding, received,
                            )?;
                            if !effects.iter().any(|effect| {
                                file.matches_effect(effect, config.run, &config.fence)
                            }) {
                                return Err(RunError::refused(
                                    "interpolation binding names no retained process effect",
                                ));
                            }
                            if request == last_request
                                && !files.iter().any(|retained| retained.same_binding(&file))
                            {
                                return Err(RunError::refused(
                                    "interpolation retry changed its owner",
                                ));
                            }
                            // The actual process receipt and synced binding above are
                            // required before replacing any hold-only descriptions.
                            prelaunch_files.retain(|_, retained| !retained.same_object(&file));
                            if !files.iter().any(|retained| retained.same_binding(&file)) {
                                if files.len() >= 64 {
                                    return Err(RunError::refused(
                                        "too many retained interpolation owners",
                                    ));
                                }
                                files.push(file);
                            }
                            last_file = Some((request, invocation));
                            request
                        }
                    };
                    last_request = request;
                    output.write_all(&request.to_be_bytes())?;
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
    if !files.is_empty() || !prelaunch_files.is_empty() {
        // Retain exact descriptions across caller loss and every Unknown. Each
        // retry re-enters identity-qualified kernel settlement; no idle lock,
        // EOF or remembered PGID grants closure or signal authority.
        while !prelaunch_files.is_empty() || !finish_interpolations(&effects, &mut files, &config)?
        {
            std::thread::sleep(Duration::from_millis(100));
        }
        retained_completed = true;
    }
    let cleanup = context(CancelToken::default(), Duration::from_secs(60))
        .cleanup(Duration::from_secs(60))
        .map_err(|_| RunError::broken("foreground cleanup overflow"))?;
    let mut residue = false;
    for effect in effects.iter().rev().filter(|_| !retained_completed) {
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

fn finish_interpolations(
    effects: &[Effect],
    files: &mut Vec<crate::interpolation::RetainedInterpolation>,
    config: &Config,
) -> Result<bool, RunError> {
    let cleanup = context(CancelToken::default(), Duration::from_secs(5))
        .cleanup(Duration::from_secs(5))
        .map_err(|_| RunError::broken("retained cleanup overflow"))?;
    let mut closed = Vec::new();
    for effect in effects {
        closed.push(
            crate::custody::settle_effect(effect, None, &cleanup, || false).0
                == EffectState::Released,
        );
    }
    let mut pending = Vec::new();
    for mut file in files.drain(..) {
        let authorized = effects.iter().zip(&closed).any(|(effect, closed)| {
            *closed && file.matches_effect(effect, config.run, &config.fence)
        });
        if !authorized || file.cleanup_after_verified_closure().is_err() {
            pending.push(file);
        }
    }
    *files = pending;
    Ok(files.is_empty() && closed.iter().all(|closed| *closed))
}

/// One registration frame's whole validation. Pure, so the request binding and
/// the retained-receipt rules are provable without a live helper. The request
/// must strictly advance: a late acknowledgement cannot answer a later frame.
pub(super) fn validate_register(
    effect: &Effect,
    request: u64,
    last_request: u64,
    last_sequence: i64,
    config: &Config,
) -> Result<(), RunError> {
    if effect.spec.kind != crate::custody::KIND
        || effect.state != EffectState::Applied
        || effect.sequence <= last_sequence
        || effect.spec.target != config.fence
    {
        return Err(RunError::broken("invalid foreground custody receipt"));
    }
    if request <= last_request {
        return Err(RunError::broken(
            "foreground custody request did not advance",
        ));
    }
    crate::custody::verify_foreground_receipt(effect, config.run)
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
