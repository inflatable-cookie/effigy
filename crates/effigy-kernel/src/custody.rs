//! Journal-first local process custody (§5.7). No signal handlers live here.
pub(crate) mod capture;
mod platform;
mod spawn;
use crate::ctx::{CleanupContext, EffectContext, OperationContext};
use crate::ident::ProcessIdentity;
use crate::observe::{AbsenceProof, Evidence, Observed, UnknownReason};
use crate::run::{Effect, EffectSpec, EffectState, EffectTicket, Run, RunError, Settlement};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::OsString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub(crate) const KIND: &str = "kernel:process";
#[derive(Clone)]
pub struct SecretValue(Vec<u8>);
impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}
impl SecretValue {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.0
    }
    pub fn new(bytes: Vec<u8>) -> io::Result<Self> {
        if bytes.is_empty() || bytes.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid secret",
            ));
        }
        Ok(Self(bytes))
    }
}
#[derive(Debug, Clone)]
pub enum EnvValue {
    Plain(OsString),
    Secret(SecretValue),
}
#[derive(Debug, Clone)]
pub enum Lifetime {
    Finite(Duration),
    Open(crate::ctx::OpenReason),
}
#[derive(Debug, Clone, Copy)]
pub enum Terminal {
    None,
    Foreground,
}
#[derive(Clone)]
pub struct ProcessSpec {
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    /// Complete spawn environment; the parent environment is never modified.
    pub env: BTreeMap<OsString, EnvValue>,
    pub grace: Duration,
    pub lifetime: Lifetime,
    pub terminal: Terminal,
    pub label: String,
}
impl std::fmt::Debug for ProcessSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcessSpec { argv/env/label: [REDACTED] }")
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessReceipt {
    pub identity: ProcessIdentity,
    pub pgid: u32,
    pub nonce: String,
    pub grace_millis: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exit {
    pub code: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Closure;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureState {
    Complete,
    Truncated(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignalDelivered {
    pub pid: u32,
    pub group: bool,
    pub signal: i32,
}
#[derive(Debug)]
pub struct StopReport {
    pub leader: Observed<Exit>,
    pub group: Observed<Closure>,
    pub capture: CaptureState,
    pub signals: Vec<SignalDelivered>,
    pub residue: Vec<UnknownReason>,
}
#[derive(Debug)]
pub enum SpawnError {
    Refused(String),
    Run(RunError),
    Io(io::Error),
    Unrecorded(RunError),
}
impl From<io::Error> for SpawnError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<RunError> for SpawnError {
    fn from(e: RunError) -> Self {
        Self::Run(e)
    }
}
#[derive(Debug)]
pub struct PartialSpawn {
    pub primary: SpawnError,
    pub cleanup: Vec<StopReport>,
}
#[derive(Debug)]
pub struct Custody;
#[derive(Debug, Clone)]
pub struct OwnedProcess {
    pub(crate) core: Arc<Mutex<ProcessCore>>,
    ticket: EffectTicket,
}
#[derive(Debug)]
pub(crate) struct ProcessCore {
    receipt: ProcessReceipt,
    log: Option<capture::Log>,
    output: VecDeque<u8>,
    queue_max: usize,
    stdout: Option<OwnedFd>,
    stderr: Option<OwnedFd>,
    redactors: [Redactor; 2],
    tail: VecDeque<u8>,
    tail_max: usize,
    capture: CaptureState,
    exit: Option<Exit>,
    reaped: bool,
    deadline: Option<Instant>,
    terminal: Option<spawn::TerminalLease>,
}
#[derive(Debug)]
struct Redactor {
    secrets: Vec<Vec<u8>>,
    pending: Vec<u8>,
}
impl Redactor {
    fn new(mut secrets: Vec<Vec<u8>>) -> Self {
        secrets.sort_by_key(|v| std::cmp::Reverse(v.len()));
        Self {
            secrets,
            pending: Vec::new(),
        }
    }
    fn push(&mut self, bytes: &[u8], eof: bool) -> Vec<u8> {
        self.pending.extend_from_slice(bytes);
        let keep = self.secrets.iter().map(Vec::len).max().unwrap_or(1);
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < self.pending.len() && (eof || self.pending.len() - pos >= keep) {
            if let Some(secret) = self
                .secrets
                .iter()
                .find(|s| self.pending[pos..].starts_with(s))
            {
                out.extend_from_slice(b"[REDACTED]");
                pos += secret.len();
            } else {
                out.push(self.pending[pos]);
                pos += 1;
            }
        }
        self.pending.drain(..pos);
        out
    }
}
fn evidence() -> Evidence {
    Evidence::new("kernel process custody".into(), None).expect("fixed source")
}
fn unknown(e: impl std::fmt::Display) -> UnknownReason {
    UnknownReason::Inconsistent(e.to_string())
}
fn lock(core: &Mutex<ProcessCore>) -> std::sync::MutexGuard<'_, ProcessCore> {
    core.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
impl Custody {
    pub fn spawn(
        run: &mut Run<'_>,
        spec: ProcessSpec,
        cx: &EffectContext,
    ) -> Result<OwnedProcess, SpawnError> {
        cx.require(crate::ctx::Capability::AdapterEffect)
            .map_err(|_| SpawnError::Refused("spawn needs Mutate".into()))?;
        if crate::store::in_transaction() {
            return Err(SpawnError::Refused("spawn inside transaction".into()));
        }
        if !run.has_sentinel() {
            return Err(SpawnError::Refused(
                "custody requires a registered sentinel".into(),
            ));
        }
        cx.operation().check("spawn").map_err(RunError::from)?;
        let secrets: Vec<Vec<u8>> = spec
            .env
            .values()
            .filter_map(|v| match v {
                EnvValue::Secret(v) => Some(v.0.clone()),
                _ => None,
            })
            .collect();
        if spec.argv.is_empty()
            || spec.argv.iter().any(|arg| {
                secrets
                    .iter()
                    .any(|s| arg.as_bytes().windows(s.len()).any(|w| w == s))
            })
        {
            return Err(SpawnError::Refused("empty argv or secret in argv".into()));
        }
        let mut redact = Redactor::new(secrets.clone());
        let label = String::from_utf8_lossy(&redact.push(spec.label.as_bytes(), true)).into_owned();
        // Only a digest and a redacted label enter the journal. Never env values.
        let digest = crate::id::Digest::of(
            &serde_json::to_vec(&spec.argv.iter().map(|s| s.as_bytes()).collect::<Vec<_>>())
                .map_err(|_| SpawnError::Refused("argv encoding".into()))?,
        );
        let ticket = run.intend_process(EffectSpec {
            target: run.fence().clone(),
            kind: KIND.into(),
            payload: serde_json::json!({"digest":digest,"label":label}),
            settlement: Settlement::Release(Value::Null),
            required: true,
        })?;
        let nonce = format!("{}:{}", run.id(), ticket.sequence());
        let mut pending = spawn::Blocked::fork(&spec, &nonce, cx.operation())?;
        let result = (|| {
            crate::fault_point!(SpawnFork)?;
            crate::fault_point!(SpawnBeforeRecord)?;
            let identity = crate::ident::process_identity(pending.pid)
                .verified()
                .cloned()
                .ok_or_else(|| SpawnError::Refused("child identity unavailable".into()))?;
            let receipt = ProcessReceipt {
                identity,
                pgid: pending.pid,
                nonce,
                grace_millis: spec.grace.as_millis().min(u128::from(u64::MAX)) as u64,
            };
            run.applied(
                &ticket,
                serde_json::to_value(&receipt)
                    .map_err(|_| SpawnError::Refused("receipt encoding".into()))?,
            )
            .map_err(SpawnError::Unrecorded)?;
            let (stdout, stderr, terminal) = pending.take_streams();
            let core = Arc::new(Mutex::new(ProcessCore {
                receipt,
                log: run
                    .capture_log(ticket.sequence(), cx.operation().limits())
                    .ok(),
                output: VecDeque::new(),
                queue_max: cx.operation().limits().queue_bytes,
                stdout: Some(stdout),
                stderr: Some(stderr),
                redactors: [Redactor::new(secrets.clone()), Redactor::new(secrets)],
                tail: VecDeque::new(),
                tail_max: cx.operation().limits().capture_tail,
                capture: CaptureState::Complete,
                exit: None,
                reaped: false,
                deadline: match spec.lifetime {
                    Lifetime::Finite(budget) => Instant::now().checked_add(budget),
                    Lifetime::Open(_) => None,
                },
                terminal,
            }));
            run.retain_process(ticket.sequence(), core.clone());
            crate::fault_point!(SpawnBeforeGo)?;
            pending.go()?;
            crate::fault_point!(SpawnAfterGo)?;
            Ok(OwnedProcess { core, ticket })
        })();
        // Blocked owns the pre-exec child until go. Its drop closes the barrier
        // and reaps it on every refusal. After go the Run owns the live core.
        result
    }
    pub fn spawn_all(
        run: &mut Run<'_>,
        specs: Vec<ProcessSpec>,
        cx: &EffectContext,
    ) -> Result<Vec<OwnedProcess>, PartialSpawn> {
        let mut children: Vec<OwnedProcess> = Vec::new();
        for spec in specs {
            match Self::spawn(run, spec, cx) {
                Ok(child) => children.push(child),
                Err(primary) => {
                    let cleanup = match cx.operation().cleanup(Duration::from_secs(30)) {
                        Ok(cx) => run.rollback_processes(&cx),
                        Err(_) => Vec::new(),
                    };
                    return Err(PartialSpawn { primary, cleanup });
                }
            }
        }
        Ok(children)
    }
}
impl OwnedProcess {
    pub fn identity(&self) -> ProcessReceipt {
        lock(&self.core).receipt.clone()
    }
    pub fn expired(&self) -> bool {
        lock(&self.core)
            .deadline
            .is_some_and(|at| Instant::now() >= at)
    }
    pub fn take_output(&self) -> Vec<u8> {
        lock(&self.core).output.drain(..).collect()
    }
    pub fn tail(&self) -> Vec<u8> {
        lock(&self.core).tail.iter().copied().collect()
    }
    pub fn poll(&mut self) -> Observed<Exit> {
        lock(&self.core).poll()
    }
    pub fn wait(&mut self, cx: &OperationContext) -> Observed<Exit> {
        loop {
            if let Err(e) = cx.check("process wait") {
                return Observed::Unknown(unknown(format!("{e:?}")));
            }
            let mut core = lock(&self.core);
            if core.deadline.is_some_and(|at| Instant::now() >= at) {
                return Observed::Unknown(UnknownReason::TimedOut {
                    phase: "workload".into(),
                });
            }
            let result = core.poll();
            if matches!(result, Observed::Verified(..)) {
                return result;
            }
            drop(core);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    pub fn complete(&self, run: &mut Run<'_>, exit: Exit) -> Result<(), RunError> {
        let core = lock(&self.core);
        if core.exit != Some(exit) {
            return Err(RunError::Invalid("unverified process exit".into()));
        }
        run.completed(
            &self.ticket,
            crate::outcome::RequiredObligation::ProcessExit(exit.code),
        )
    }
    pub fn forward(&mut self, signal: i32) -> Observed<SignalDelivered> {
        lock(&self.core).signal(signal)
    }
    pub fn stop(&mut self, cx: &CleanupContext) -> StopReport {
        lock(&self.core).stop(cx)
    }
}
impl ProcessCore {
    fn drain(&mut self) {
        for index in 0..2 {
            let fd = if index == 0 {
                &mut self.stdout
            } else {
                &mut self.stderr
            };
            let Some(handle) = fd.as_ref() else {
                continue;
            };
            let mut buf = [0u8; 8192];
            let mut eof = false;
            // Bound each turn even if a child writes continuously.
            for _ in 0..16 {
                // SAFETY: live nonblocking read fd and writable stack buffer.
                let count =
                    unsafe { libc::read(handle.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) };
                if count == 0 {
                    eof = true;
                    break;
                }
                if count < 0 {
                    let e = io::Error::last_os_error();
                    if e.kind() != io::ErrorKind::WouldBlock
                        && e.kind() != io::ErrorKind::Interrupted
                    {
                        self.capture = CaptureState::Truncated("capture read failed".into());
                        eof = true;
                    }
                    break;
                }
                let clean = self.redactors[index].push(&buf[..count as usize], false);
                if let Some(log) = self.log.as_mut() {
                    if log.append(&clean).is_err() {
                        self.capture = CaptureState::Truncated("log write failed".into());
                        self.log = None;
                    }
                }
                self.output.extend(clean.iter().copied());
                while self.output.len() > self.queue_max {
                    self.output.pop_front();
                }
                self.tail.extend(clean);
                while self.tail.len() > self.tail_max {
                    self.tail.pop_front();
                }
            }
            if eof {
                let clean = self.redactors[index].push(&[], true);
                if let Some(log) = self.log.as_mut() {
                    if log.append(&clean).is_err() {
                        self.capture = CaptureState::Truncated("log write failed".into());
                        self.log = None;
                    }
                }
                self.output.extend(clean.iter().copied());
                while self.output.len() > self.queue_max {
                    self.output.pop_front();
                }
                self.tail.extend(clean);
                while self.tail.len() > self.tail_max {
                    self.tail.pop_front();
                }
                *fd = None;
            }
        }
    }
    fn poll(&mut self) -> Observed<Exit> {
        self.drain();
        if let Some(exit) = self.exit {
            return Observed::Verified(exit, evidence());
        }
        match spawn::peek(self.receipt.identity.pid) {
            Ok(Some(exit)) => {
                self.exit = Some(exit);
                Observed::Verified(exit, evidence())
            }
            Ok(None) => Observed::Unknown(UnknownReason::Inconsistent("leader running".into())),
            Err(e) => Observed::Unknown(unknown(e)),
        }
    }
    fn signal(&mut self, signal: i32) -> Observed<SignalDelivered> {
        if self.reaped {
            return Observed::Unknown(unknown("leader already reaped"));
        }
        // The unreaped direct child pins both leader pid and pgid. No numeric
        // signal is issued after that evidence is relinquished.
        platform::owner_signal(&self.receipt, signal)
    }
    pub(crate) fn stop(&mut self, cx: &CleanupContext) -> StopReport {
        let mut signals = Vec::new();
        let mut residue = Vec::new();
        if !self.reaped {
            for signal in [libc::SIGTERM, libc::SIGCONT] {
                match self.signal(signal) {
                    Observed::Verified(sent, _) => signals.push(sent),
                    Observed::Unknown(r) => residue.push(r),
                    _ => {}
                }
            }
        }
        let grace = Instant::now()
            .checked_add(Duration::from_millis(self.receipt.grace_millis))
            .unwrap_or_else(Instant::now);
        let mut killed = false;
        let group = loop {
            self.poll();
            let state = platform::group_closed(
                self.receipt.pgid,
                self.exit.map(|_| self.receipt.identity.pid),
            );
            if matches!(state, Observed::Verified(..) | Observed::Absent(_)) {
                break state;
            }
            if let Err(e) = cx.operation().check("group closure") {
                break Observed::Unknown(unknown(format!("{e:?}")));
            }
            if Instant::now() >= grace && !killed && !self.reaped {
                match self.signal(libc::SIGKILL) {
                    Observed::Verified(sent, _) => signals.push(sent),
                    Observed::Unknown(r) => residue.push(r),
                    _ => {}
                }
                killed = true;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        if matches!(group, Observed::Verified(..) | Observed::Absent(_)) && !self.reaped {
            // Closure is established first. Only now may the direct child be reaped.
            if let Ok(Some(exit)) = spawn::reap(self.receipt.identity.pid) {
                self.exit = Some(exit);
                self.reaped = true;
                self.terminal.take();
            }
        }
        while self.stdout.is_some() || self.stderr.is_some() {
            self.drain();
            if cx.operation().check("capture drain").is_err() {
                self.capture = CaptureState::Truncated("drain deadline".into());
                self.stdout = None;
                self.stderr = None;
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        if let Observed::Unknown(reason) = &group {
            residue.push(reason.clone());
        }
        StopReport {
            leader: self
                .exit
                .map(|e| Observed::Verified(e, evidence()))
                .unwrap_or_else(|| Observed::Unknown(unknown("leader not closed"))),
            group,
            capture: self.capture.clone(),
            signals,
            residue,
        }
    }
}
pub(crate) fn settle_effect(
    effect: &Effect,
    live: Option<Arc<Mutex<ProcessCore>>>,
    cx: &CleanupContext,
) -> (EffectState, Option<String>) {
    let receipt = match effect
        .receipt
        .as_ref()
        .map(|v| serde_json::from_value::<ProcessReceipt>(v.clone()))
    {
        Some(Ok(r)) => r,
        None => return (EffectState::Released, None), // no receipt: barrier never released
        _ => return (EffectState::Residue, Some("invalid custody receipt".into())),
    };
    let observed = if let Some(core) = live {
        lock(&core).stop(cx).group
    } else {
        platform::recover(&receipt, cx)
    };
    match observed {
        Observed::Verified(..) | Observed::Absent(_) => (EffectState::Released, None),
        Observed::Unknown(reason) => (EffectState::Residue, Some(format!("{reason:?}"))),
    }
}
