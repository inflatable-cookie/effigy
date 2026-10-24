//! In-memory backup custody. Registration is acknowledged before the go byte.
use super::{check, evidence, failure};
use crate::ctx::{CancelToken, Capability, EffectClass, EffectContext, ObserveContext};
use crate::custody::{platform, ProcessReceipt, SignalDelivered};
use crate::ident::{self, ProcessIdentity, ProcessState};
use crate::observe::{Observed, UnknownReason};
use crate::supervise::Executable;
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::process::{Child, ChildStdin, ChildStdout, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, MutexGuard, TryLockError,
};
use std::time::Duration;

pub(crate) const GRACE: Duration = Duration::from_millis(100);
const FRAME_MAX: usize = 16 * 1024;
static INSTALLED: AtomicBool = AtomicBool::new(false);

/// Installed once by binary main, like the signal router. Contexts share this
/// service explicitly; it carries no operation context or process-global env.
#[derive(Debug)]
pub struct ProbeRuntime {
    executable: Executable,
    observer: ProcessIdentity,
    pub(super) client: Mutex<Option<Client>>,
}
impl ProbeRuntime {
    pub fn install(executable: Executable) -> io::Result<Arc<Self>> {
        executable.verify()?;
        let observer = verified(ident::process_identity(std::process::id()))?;
        if INSTALLED.swap(true, Ordering::AcqRel) {
            return Err(io::Error::other("probe runtime already installed"));
        }
        Ok(Arc::new(Self {
            executable,
            observer,
            client: Mutex::new(None),
        }))
    }
    /// Pure diagnostics for the current backup; never starts or repairs it.
    pub fn guard_identity(&self, cx: &ObserveContext) -> Observed<ProcessIdentity> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        match self.lock(cx) {
            Ok(mut slot) => match slot.as_mut() {
                Some(client) => {
                    if client.alive() {
                        Observed::Verified(client.identity.clone(), evidence())
                    } else {
                        Observed::Unknown(UnknownReason::GuardUnavailable)
                    }
                }
                _ => Observed::Unknown(UnknownReason::GuardUnavailable),
            },
            Err(reason) => Observed::Unknown(reason),
        }
    }
    pub(super) fn lock(
        &self,
        cx: &ObserveContext,
    ) -> Result<MutexGuard<'_, Option<Client>>, UnknownReason> {
        loop {
            check(cx, "probe admission")?;
            match self.client.try_lock() {
                Ok(client) => return Ok(client),
                Err(TryLockError::Poisoned(_)) => return Err(UnknownReason::GuardUnavailable),
                Err(TryLockError::WouldBlock) => {
                    std::thread::sleep(cx.remaining().min(Duration::from_millis(2)))
                }
            }
        }
    }
    pub(super) fn start(&self, cx: &ObserveContext) -> io::Result<Client> {
        if ident::process_identity(std::process::id()).verified() != Some(&self.observer) {
            return Err(io::Error::other("probe runtime inherited across fork"));
        }
        let mut command = crate::supervise::helper(&self.executable, "__kernel-probe-guard")?;
        command.stdout(Stdio::piped());
        let mut child = command.spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("missing guard input"))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("missing guard output"))?;
        let identity = match verified(ident::process_identity(child.id())) {
            Ok(id) => id,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut client = Client {
            child,
            input,
            output,
            identity,
            sequence: 0,
            buffered: Vec::new(),
        };
        nonblocking(client.input.as_raw_fd())?;
        nonblocking(client.output.as_raw_fd())?;
        client.send(
            &Message::Hello {
                executable: self.executable.clone(),
                observer: self.observer.clone(),
            },
            cx,
        )?;
        if client.ack(cx)? != Ack::Ready {
            return Err(io::Error::other("invalid guard startup acknowledgement"));
        }
        Ok(client)
    }
}
impl ObserveContext {
    pub fn with_probes(mut self, runtime: Arc<ProbeRuntime>) -> Self {
        self.probes = Some(runtime);
        self
    }
}
#[derive(Debug)]
pub(super) struct Client {
    child: Child,
    input: ChildStdin,
    output: ChildStdout,
    pub identity: ProcessIdentity,
    sequence: u64,
    buffered: Vec<u8>,
}
impl Client {
    pub fn nonce(&mut self, observer: ProcessIdentity) -> io::Result<Nonce> {
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("probe sequence exhausted"))?;
        Ok(Nonce {
            observer,
            guard: self.identity.clone(),
            sequence: self.sequence,
        })
    }
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
    pub fn send(&mut self, message: &Message, cx: &ObserveContext) -> io::Result<()> {
        write_frame(&mut self.input, message, cx)
    }
    pub fn ack(&mut self, cx: &ObserveContext) -> io::Result<Ack> {
        loop {
            check(cx, "guard acknowledgement").map_err(uncertain)?;
            match read_frame(&mut self.output, &mut self.buffered)? {
                Frame::Value(ack) => return Ok(ack),
                Frame::Eof => return Err(io::Error::other("guard closed acknowledgement pipe")),
                Frame::Pending => std::thread::sleep(cx.remaining().min(Duration::from_millis(2))),
            }
        }
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        // All primary probes have finished before releasing this serialized
        // service. A helper is our direct child; kill/wait only this pid.
        let _ = self.child.kill();
        let cx = guard_context(CancelToken::default());
        while matches!(self.child.try_wait(), Ok(None)) && cx.check("guard reap").is_ok() {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Nonce {
    pub observer: ProcessIdentity,
    pub guard: ProcessIdentity,
    pub sequence: u64,
}
impl Nonce {
    pub fn encode(&self) -> io::Result<String> {
        serde_json::to_string(self).map_err(io::Error::other)
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) enum Message {
    Hello {
        executable: Executable,
        observer: ProcessIdentity,
    },
    Register {
        receipt: ProcessReceipt,
        deadline_nanos: u128,
    },
    Closed {
        sequence: u64,
    },
}
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Ack {
    Ready,
    Registered { group_capability: bool },
    Closed,
}
#[derive(Debug)]
struct Registration {
    receipt: ProcessReceipt,
    deadline_nanos: u128,
    sequence: u64,
    // Opened at registration, never re-opened for descriptor group signalling.
    group: Option<OwnedFd>,
}
fn uncertain(reason: UnknownReason) -> io::Error {
    io::Error::other(format!("{reason}"))
}
fn verified<T>(value: Observed<T>) -> io::Result<T> {
    match value {
        Observed::Verified(v, _) => Ok(v),
        other => {
            let _ = other;
            Err(io::Error::other("process identity unavailable"))
        }
    }
}
pub(super) fn nonblocking(fd: RawFd) -> io::Result<()> {
    // SAFETY: borrowed live descriptor; only its status flags change.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    // SAFETY: borrowed live descriptor and preserved flags plus O_NONBLOCK.
    if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
fn write_frame(
    writer: &mut impl Write,
    value: &impl Serialize,
    cx: &ObserveContext,
) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() > FRAME_MAX {
        return Err(io::Error::other("guard frame too large"));
    }
    bytes.push(b'\n');
    let mut offset = 0;
    while offset < bytes.len() {
        check(cx, "guard write").map_err(uncertain)?;
        match writer.write(&bytes[offset..]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "guard pipe closed",
                ))
            }
            Ok(n) => offset += n,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                std::thread::sleep(cx.remaining().min(Duration::from_millis(2)))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
enum Frame<T> {
    Value(T),
    Eof,
    Pending,
}
fn read_frame<T: serde::de::DeserializeOwned>(
    reader: &mut impl Read,
    buffered: &mut Vec<u8>,
) -> io::Result<Frame<T>> {
    loop {
        if let Some(end) = buffered.iter().position(|b| *b == b'\n') {
            let value = serde_json::from_slice(&buffered[..end]).map_err(io::Error::other)?;
            buffered.drain(..=end);
            return Ok(Frame::Value(value));
        }
        if buffered.len() > FRAME_MAX {
            return Err(io::Error::other("guard frame too large"));
        }
        let mut bytes = [0; 1024];
        match reader.read(&mut bytes) {
            Ok(0) if buffered.is_empty() => return Ok(Frame::Eof),
            Ok(0) => return Err(io::Error::other("truncated guard frame")),
            Ok(n) => buffered.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(Frame::Pending),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}
fn clock() -> io::Result<u128> {
    Ok(verified(ident::boot_clock())?.elapsed.as_nanos())
}
pub(super) fn deadline(cx: &ObserveContext) -> io::Result<u128> {
    clock()?
        .checked_add(cx.remaining().as_nanos())
        .ok_or_else(|| io::Error::other("probe deadline overflow"))
}
/// Hidden same-binary entry point, invoked after main installs its router.
pub fn internal_entry(entry: &str, cancel: CancelToken) -> Option<io::Result<()>> {
    (entry == "__kernel-probe-guard").then(|| guard_main(cancel))
}
fn guard_context(cancel: CancelToken) -> ObserveContext {
    use crate::ctx::*;
    OperationContext::new(
        Deadline::At(std::time::Instant::now() + Duration::from_secs(5)),
        cancel,
        Limits {
            capture_tail: 0,
            log_rotate: 0,
            log_keep: 0,
            line_max: FRAME_MAX,
            queue_bytes: FRAME_MAX,
            probe_concurrency: 1,
        },
        EffectClass::Observe,
        Placement::Host,
    )
    .observe()
    .expect("finite Observe")
}
fn guard_main(cancel: CancelToken) -> io::Result<()> {
    let mut input = io::stdin();
    let mut output = io::stdout();
    nonblocking(input.as_raw_fd())?;
    nonblocking(output.as_raw_fd())?;
    let mut buffered = Vec::new();
    let startup = guard_context(cancel.clone());
    let (observer, own) = loop {
        check(&startup, "guard startup").map_err(uncertain)?;
        match read_frame(&mut input, &mut buffered)? {
            Frame::Value(Message::Hello {
                executable,
                observer,
            }) => {
                executable.verify()?;
                // Both identities must be on this boot. The pipe, not a pid
                // or heartbeat, supplies observer liveness after startup.
                if ident::process_identity(observer.pid).verified() != Some(&observer) {
                    return Err(io::Error::other("observer changed"));
                }
                break (
                    observer,
                    verified(ident::process_identity(std::process::id()))?,
                );
            }
            Frame::Pending => std::thread::sleep(Duration::from_millis(2)),
            _ => return Err(io::Error::other("missing guard hello")),
        }
    };
    write_frame(&mut output, &Ack::Ready, &startup)?;
    let mut open: Option<Registration> = None;
    let result = (|| loop {
        if cancel.cause().is_some() {
            return Ok(());
        }
        if let Some(registration) = &open {
            if clock()? >= registration.deadline_nanos.saturating_add(GRACE.as_nanos()) {
                stop_registration(registration);
            }
        }
        match read_frame(&mut input, &mut buffered)? {
            Frame::Eof => return Ok(()),
            Frame::Pending => std::thread::sleep(Duration::from_millis(2)),
            Frame::Value(Message::Register {
                receipt,
                deadline_nanos,
            }) if open.is_none() => {
                let nonce: Nonce =
                    serde_json::from_str(&receipt.nonce).map_err(io::Error::other)?;
                if nonce.observer != observer
                    || nonce.guard != own
                    || receipt.pgid != receipt.identity.pid
                    || nonce.sequence == 0
                    || ident::process_identity(receipt.identity.pid).verified()
                        != Some(&receipt.identity)
                {
                    return Err(io::Error::other("invalid probe registration"));
                }
                let group = group_capability(receipt.identity.pid)?;
                let capable = group.is_some();
                open = Some(Registration {
                    receipt,
                    deadline_nanos,
                    sequence: nonce.sequence,
                    group,
                });
                crate::fault_point!(ProbeGuardAck)?;
                write_frame(
                    &mut output,
                    &Ack::Registered {
                        group_capability: capable,
                    },
                    &guard_context(cancel.clone()),
                )?;
            }
            Frame::Value(Message::Closed { sequence })
                if open.as_ref().is_some_and(|r| r.sequence == sequence) =>
            {
                open = None;
                write_frame(&mut output, &Ack::Closed, &guard_context(cancel.clone()))?;
            }
            _ => return Err(io::Error::other("unmodelled guard message")),
        }
    })();
    // Even malformed input or a failed acknowledgement retains backup custody.
    if let Some(registration) = open {
        stop_registration(&registration);
    }
    result
}
#[cfg(target_os = "linux")]
fn group_capability(pid: u32) -> io::Result<Option<OwnedFd>> {
    group_capability_with(pid, |fd| {
        // SAFETY: registered live leader pidfd, signal zero and null siginfo.
        // Flag 4 requests the original group, never a numeric fallback.
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                0,
                std::ptr::null::<libc::siginfo_t>(),
                4u32,
            )
        } == 0
        {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    })
}
#[cfg(target_os = "linux")]
fn group_capability_with(
    pid: u32,
    signal_zero: impl FnOnce(&OwnedFd) -> io::Result<()>,
) -> io::Result<Option<OwnedFd>> {
    let fd = platform::pidfd(pid)?;
    match signal_zero(&fd) {
        Ok(()) => Ok(Some(fd)),
        Err(error) if matches!(error.raw_os_error(), Some(libc::EINVAL | libc::ENOSYS)) => Ok(None),
        Err(error) => Err(error),
    }
}
#[cfg(target_os = "macos")]
fn group_capability(_: u32) -> io::Result<Option<OwnedFd>> {
    Ok(None)
}
trait Backup {
    fn descriptor_group(&mut self) -> Option<Observed<SignalDelivered>>;
    fn leader(&mut self) -> Observed<ProcessIdentity>;
    fn numeric_group(&mut self) -> Observed<SignalDelivered>;
    fn nonce_members(&mut self);
}
fn stop_with(backup: &mut impl Backup, expected: &ProcessIdentity) {
    // A descriptor observation, even Unknown or ESRCH, never falls back to a
    // numeric signal. Its capability refers only to the registered group.
    if backup.descriptor_group().is_none()
        && matches!(backup.leader(), Observed::Verified(ref identity, _) if identity == expected)
    {
        backup.numeric_group();
    }
    backup.nonce_members();
}
struct NativeBackup<'a>(&'a Registration);
impl Backup for NativeBackup<'_> {
    fn descriptor_group(&mut self) -> Option<Observed<SignalDelivered>> {
        self.0.group.as_ref()?;
        #[cfg(target_os = "linux")]
        if let Some(fd) = &self.0.group {
            // SAFETY: original retained pidfd, valid signal and null siginfo;
            // group flag targets the kernel group, never a reused number.
            let result = unsafe {
                libc::syscall(
                    libc::SYS_pidfd_send_signal,
                    fd.as_raw_fd(),
                    libc::SIGKILL,
                    std::ptr::null::<libc::siginfo_t>(),
                    4u32,
                )
            };
            return Some(if result == 0 {
                Observed::Verified(
                    SignalDelivered {
                        pid: self.0.receipt.pgid,
                        group: true,
                        signal: libc::SIGKILL,
                    },
                    evidence(),
                )
            } else {
                Observed::Unknown(failure(io::Error::last_os_error()))
            });
        }
        None
    }
    fn leader(&mut self) -> Observed<ProcessIdentity> {
        match ident::process_state(self.0.receipt.identity.pid) {
            Observed::Verified(ProcessState::Active | ProcessState::Stopped, _) => {
                ident::process_identity(self.0.receipt.identity.pid)
            }
            Observed::Absent(proof) => Observed::Absent(proof),
            Observed::Unknown(reason) => Observed::Unknown(reason),
            _ => Observed::Unknown(UnknownReason::Inconsistent("leader exited".into())),
        }
    }
    fn numeric_group(&mut self) -> Observed<SignalDelivered> {
        // Recheck immediately at the native check/signal boundary.
        if platform::generation(&self.0.receipt.identity)
            .verified()
            .is_none()
        {
            return Observed::Unknown(UnknownReason::Inconsistent("leader changed".into()));
        }
        platform::numeric(self.0.receipt.pgid, true, libc::SIGKILL)
    }
    fn nonce_members(&mut self) {
        stop_nonce(&self.0.receipt);
    }
}
fn stop_registration(registration: &Registration) {
    stop_with(
        &mut NativeBackup(registration),
        &registration.receipt.identity,
    );
}
/// Signals only individual nonce-carrying processes, including new sessions.
/// Every call rechecks identity and environment, opening a pidfd first on Linux.
pub(super) fn stop_nonce(receipt: &ProcessReceipt) -> Vec<Observed<SignalDelivered>> {
    let pids = match platform::group_pids(0) {
        Ok(pids) => pids,
        Err(e) => return vec![Observed::Unknown(failure(e))],
    };
    let mut signals = Vec::new();
    for pid in pids {
        if platform::read_nonce_key(pid, b"EFFIGY_PROBE_NONCE=").verified() != Some(&receipt.nonce)
        {
            continue;
        }
        if let Observed::Verified(identity, _) = ident::process_identity(pid) {
            if platform::not_earlier(&identity, receipt) {
                signals.push(signal_nonce(&identity, receipt));
            }
        }
    }
    signals
}
fn signal_nonce(identity: &ProcessIdentity, receipt: &ProcessReceipt) -> Observed<SignalDelivered> {
    #[cfg(target_os = "linux")]
    let fd = match platform::pidfd(identity.pid) {
        Ok(fd) => fd,
        Err(e) => return Observed::Unknown(failure(e)),
    };
    if platform::generation(identity).verified().is_none()
        || platform::read_nonce_key(identity.pid, b"EFFIGY_PROBE_NONCE=").verified()
            != Some(&receipt.nonce)
        || !platform::not_earlier(identity, receipt)
    {
        return Observed::Unknown(UnknownReason::Inconsistent(
            "probe generation unverified".into(),
        ));
    }
    #[cfg(target_os = "linux")]
    {
        // SAFETY: per-process pidfd opened before fresh identity/nonce checks.
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                fd.as_raw_fd(),
                libc::SIGKILL,
                std::ptr::null::<libc::siginfo_t>(),
                0u32,
            )
        } != 0
        {
            return Observed::Unknown(failure(io::Error::last_os_error()));
        }
        Observed::Verified(
            SignalDelivered {
                pid: identity.pid,
                group: false,
                signal: libc::SIGKILL,
            },
            evidence(),
        )
    }
    #[cfg(target_os = "macos")]
    platform::numeric(identity.pid, false, libc::SIGKILL)
}
#[derive(Debug)]
pub struct MaintenanceReport {
    pub signals: Vec<Observed<SignalDelivered>>,
    pub uncertain: Vec<UnknownReason>,
}
/// Explicit Mutate maintenance; Observe cannot reach this API. No state writes
/// or group signals: only a valid nonce whose two exact owners are absent.
pub fn maintain(cx: &EffectContext) -> Result<MaintenanceReport, UnknownReason> {
    if cx.operation().class() != EffectClass::Mutate
        || cx.require(Capability::HostAuthority).is_err()
    {
        return Err(UnknownReason::HostAuthorityRequired);
    }
    let pids = platform::group_pids(0).map_err(failure)?;
    let mut report = MaintenanceReport {
        signals: Vec::new(),
        uncertain: Vec::new(),
    };
    for pid in pids {
        cx.operation()
            .check("probe maintenance")
            .map_err(|e| UnknownReason::Inconsistent(format!("{e:?}")))?;
        let Some(nonce) = platform::read_nonce_key(pid, b"EFFIGY_PROBE_NONCE=")
            .verified()
            .cloned()
        else {
            continue;
        };
        if nonce.len() > 2048 {
            continue;
        }
        let Ok(owners) = serde_json::from_str::<Nonce>(&nonce) else {
            continue;
        };
        if owners.sequence == 0 || !owner_absent(&owners.observer) || !owner_absent(&owners.guard) {
            continue;
        }
        let identity = match ident::process_identity(pid) {
            Observed::Verified(id, _) => id,
            Observed::Unknown(r) => {
                report.uncertain.push(r);
                continue;
            }
            _ => continue,
        };
        let observer_receipt = ProcessReceipt {
            identity: owners.observer,
            pgid: pid,
            nonce,
            grace_millis: 0,
        };
        let guard_receipt = ProcessReceipt {
            identity: owners.guard,
            ..observer_receipt.clone()
        };
        if platform::not_earlier(&identity, &observer_receipt)
            && platform::not_earlier(&identity, &guard_receipt)
        {
            report.signals.push(signal_nonce(&identity, &guard_receipt));
        }
    }
    Ok(report)
}
fn owner_absent(owner: &ProcessIdentity) -> bool {
    match ident::process_identity(owner.pid) {
        Observed::Absent(_) => true,
        Observed::Verified(current, _) if current != *owner => true,
        Observed::Verified(_, _) => matches!(
            ident::process_state(owner.pid),
            Observed::Verified(ProcessState::Zombie, _)
        ),
        Observed::Unknown(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observe::AbsenceProof;
    struct Strict {
        descriptor: Option<Observed<SignalDelivered>>,
        leader: Option<Observed<ProcessIdentity>>,
        numeric: usize,
        nonce: usize,
    }
    impl Backup for Strict {
        fn descriptor_group(&mut self) -> Option<Observed<SignalDelivered>> {
            self.descriptor.take()
        }
        fn leader(&mut self) -> Observed<ProcessIdentity> {
            self.leader.take().expect("unmodelled leader lookup")
        }
        fn numeric_group(&mut self) -> Observed<SignalDelivered> {
            self.numeric += 1;
            Observed::Unknown(UnknownReason::Injected)
        }
        fn nonce_members(&mut self) {
            self.nonce += 1;
        }
    }
    #[cfg(feature = "fault-injection")]
    #[test]
    fn probe_unknown_owner_never_authorizes_maintenance() {
        let owner = ident::process_identity(std::process::id())
            .verified()
            .unwrap()
            .clone();
        crate::fault::set_unknown(true);
        assert!(!owner_absent(&owner));
        crate::fault::set_unknown(false);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn probe_registration_einval_selects_only_the_fallback_capability() {
        for code in [libc::EINVAL, libc::ENOSYS] {
            assert!(group_capability_with(std::process::id(), |_| Err(
                io::Error::from_raw_os_error(code)
            ))
            .unwrap()
            .is_none());
        }
        assert!(
            group_capability_with(std::process::id(), |_| Err(io::Error::from_raw_os_error(
                libc::EPERM
            )))
            .is_err()
        );
    }
    #[test]
    fn probe_backup_descriptor_never_falls_back_to_reused_numeric_group() {
        let identity = ident::process_identity(std::process::id())
            .verified()
            .unwrap()
            .clone();
        for result in [
            Observed::Absent(AbsenceProof::GroupEmpty { pgid: identity.pid }),
            Observed::Unknown(UnknownReason::Injected),
        ] {
            let mut fake = Strict {
                descriptor: Some(result),
                leader: None,
                numeric: 0,
                nonce: 0,
            };
            stop_with(&mut fake, &identity);
            assert_eq!((fake.numeric, fake.nonce), (0, 1));
        }
    }
    #[test]
    fn probe_backup_unavailable_capability_requires_live_exact_leader() {
        let identity = ident::process_identity(std::process::id())
            .verified()
            .unwrap()
            .clone();
        let mut foreign = identity.clone();
        foreign.pid += 1;
        for (leader, numeric) in [
            (Observed::Unknown(UnknownReason::Injected), 0),
            (
                Observed::Absent(AbsenceProof::NoSuchProcess { pid: identity.pid }),
                0,
            ),
            (Observed::Verified(foreign, evidence()), 0),
            (Observed::Verified(identity.clone(), evidence()), 1),
        ] {
            let mut fake = Strict {
                descriptor: None,
                leader: Some(leader),
                numeric: 0,
                nonce: 0,
            };
            stop_with(&mut fake, &identity);
            assert_eq!((fake.numeric, fake.nonce), (numeric, 1));
        }
    }
}
