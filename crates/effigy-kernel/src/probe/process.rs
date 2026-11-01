use super::guard::{self, Ack, Client, Message};
use super::{check, evidence, failure};
use crate::ctx::{CleanupContext, ObserveContext};
use crate::custody::{
    platform, spawn, EnvValue, Lifetime, ProcessReceipt, ProcessSpec, Redactor, Terminal,
};
use crate::ident;
use crate::observe::{Observed, UnknownReason};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};
use std::path::PathBuf;
use std::time::Duration;

// Signal grace bounds workload lifetime. Closure verification also scans native
// identities, reaps and drains buffered output, so it has its own finite budget.
const CLOSURE_BUDGET: Duration = Duration::from_secs(5);

/// Adapters declare this command non-mutating. A complete environment is passed
/// explicitly; stdin is EOF and there is no terminal, state root, or journal.
#[derive(Clone)]
pub struct ProbeCommand {
    pub argv: Vec<OsString>,
    pub cwd: PathBuf,
    pub env: BTreeMap<OsString, EnvValue>,
    pub deadline: Duration,
    pub max_output: usize,
}
impl std::fmt::Debug for ProbeCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProbeCommand { argv/env: [REDACTED] }")
    }
}
#[derive(Debug)]
pub struct ProbeOutput {
    /// Merged, redacted stdout/stderr, bounded by both command and context.
    pub output: Vec<u8>,
    pub exit: crate::custody::Exit,
    pub truncated: bool,
    pub warnings: Vec<UnknownReason>,
}
impl ObserveContext {
    pub fn probe_process(&self, command: ProbeCommand) -> Observed<ProbeOutput> {
        #[cfg(all(test, feature = "fault-injection"))]
        if let Some(injected) = crate::fault::injected() {
            return injected;
        }
        let result = (|| {
            crate::fault_point!(ProbeObservation).map_err(failure)?;
            check(self, "probe process")?;
            if self.limits().probe_concurrency == 0 {
                return Err(UnknownReason::Inconsistent(
                    "probe concurrency is zero".into(),
                ));
            }
            if crate::store::in_transaction() {
                return Err(UnknownReason::Inconsistent(
                    "probe inside transaction".into(),
                ));
            }
            let operation = self.operation.child(command.deadline);
            let mut cx = operation
                .observe()
                .map_err(|_| UnknownReason::GuardUnavailable)?;
            cx.probes = self.probes.clone();
            let runtime = cx.probes.as_ref().ok_or(UnknownReason::GuardUnavailable)?;
            let mut slot = runtime.lock(&cx)?;
            if slot.as_mut().is_some_and(|c| !c.alive()) {
                *slot = None;
            }
            if slot.is_none() {
                *slot = Some(
                    runtime
                        .start(&cx)
                        .map_err(|_| UnknownReason::GuardUnavailable)?,
                );
            }
            let client = slot.as_mut().expect("started guard");
            let result = execute(command, &cx, client);
            // A failed exchange poisons registration state. Drop the helper so
            // the next call starts a fresh guard and acknowledges it first.
            if result.is_err()
                || result
                    .as_ref()
                    .is_ok_and(|output| output.warnings.contains(&UnknownReason::GuardUnavailable))
            {
                *slot = None;
            }
            result
        })();
        match result {
            Ok(value) => Observed::Verified(value, evidence()),
            Err(reason) => Observed::Unknown(reason),
        }
    }
}
fn execute(
    command: ProbeCommand,
    cx: &ObserveContext,
    client: &mut Client,
) -> Result<ProbeOutput, UnknownReason> {
    let observer = ident::process_identity(std::process::id())
        .verified()
        .cloned()
        .ok_or(UnknownReason::GuardUnavailable)?;
    let nonce = client.nonce(observer).map_err(failure)?;
    let nonce_text = nonce.encode().map_err(failure)?;
    let spec = ProcessSpec {
        argv: command.argv,
        cwd: command.cwd,
        env: command.env,
        grace: guard::GRACE,
        lifetime: Lifetime::Finite(command.deadline),
        terminal: Terminal::None,
        label: "read-only probe".into(),
    };
    let secrets = spec.validated_secrets().map_err(failure)?;
    crate::fault_point!(ProbeFork).map_err(failure)?;
    let mut blocked =
        spawn::Blocked::fork_with_nonce(&spec, "EFFIGY_PROBE_NONCE", &nonce_text, &cx.operation)
            .map_err(failure)?;
    let identity = ident::process_identity(blocked.pid)
        .verified()
        .cloned()
        .ok_or(UnknownReason::GuardUnavailable)?;
    let receipt = ProcessReceipt {
        identity,
        pgid: blocked.pid,
        nonce: nonce_text,
        grace_millis: guard::GRACE.as_millis() as u64,
    };
    crate::fault_point!(ProbeRegister).map_err(failure)?;
    client
        .send(
            &Message::Register {
                receipt: receipt.clone(),
                deadline_nanos: guard::deadline(cx).map_err(failure)?,
            },
            cx,
        )
        .map_err(|_| UnknownReason::GuardUnavailable)?;
    if !matches!(client.ack(cx), Ok(Ack::Registered { .. })) {
        return Err(UnknownReason::GuardUnavailable);
    }
    crate::fault_point!(ProbeBeforeGo).map_err(failure)?;
    // From this point the pin owns the direct child, including error paths. It
    // never reaps until the group is closed or its finite cleanup budget ends.
    blocked.go().map_err(failure)?;
    let mut pin = Pin {
        receipt,
        reaped: false,
    };
    let executed = blocked.executed(&cx.operation);
    let (stdout, stderr, _) = blocked.take_streams();
    let mut capture = Capture {
        stdout: Some(stdout),
        stderr: Some(stderr),
        redactor: Redactor::new(secrets),
        bytes: Vec::new(),
        max: command.max_output.min(cx.limits().capture_tail),
        truncated: false,
    };
    let work = (|| {
        executed.map_err(|_| {
            check(cx, "probe exec")
                .err()
                .unwrap_or_else(|| UnknownReason::ToolUnavailable("probe executable".into()))
        })?;
        crate::fault_point!(ProbeAfterGo).map_err(failure)?;
        loop {
            check(cx, "probe process")?;
            capture.drain().map_err(failure)?;
            if let Some(exit) = spawn::peek(pin.receipt.identity.pid).map_err(failure)? {
                // Leader exit alone never proves process or stream closure.
                if matches!(
                    platform::group_closed(pin.receipt.pgid, &[&pin.receipt.identity]),
                    Observed::Verified(..) | Observed::Absent(_)
                ) {
                    return Ok(exit);
                }
            }
            std::thread::sleep(cx.remaining().min(Duration::from_millis(2)));
        }
    })();
    let cleanup = cx
        .operation
        .cleanup(CLOSURE_BUDGET)
        .map_err(|_| UnknownReason::GuardUnavailable)?;
    let close_fault = crate::fault_point!(ProbeCloseBefore);
    let closed = pin.close(&cleanup, &mut capture);
    let mut warnings = Vec::new();
    if !client.alive() {
        warnings.push(UnknownReason::GuardUnavailable);
    }
    if closed {
        // Use the independent, finite cleanup budget after cancellation/expiry.
        let close_cx = cx
            .operation
            .cleanup(guard::GRACE)
            .map_err(|_| UnknownReason::GuardUnavailable)?
            .operation()
            .observe()
            .map_err(|_| UnknownReason::GuardUnavailable)?;
        if client
            .send(
                &Message::Closed {
                    sequence: nonce.sequence,
                },
                &close_cx,
            )
            .is_err()
            || !matches!(client.ack(&close_cx), Ok(Ack::Closed))
        {
            warnings.push(UnknownReason::GuardUnavailable);
        }
    }
    let exit = work?;
    close_fault.map_err(failure)?;
    if !closed {
        return Err(UnknownReason::TimedOut {
            phase: "probe closure".into(),
        });
    }
    Ok(ProbeOutput {
        output: capture.bytes,
        exit,
        truncated: capture.truncated,
        warnings,
    })
}
struct Pin {
    receipt: ProcessReceipt,
    reaped: bool,
}
impl Pin {
    fn close(&mut self, cx: &CleanupContext, capture: &mut Capture) -> bool {
        platform::owner_signal(&self.receipt, libc::SIGKILL);
        guard::stop_nonce(&self.receipt);
        loop {
            let moved = match capture.drain() {
                Ok(moved) => moved,
                Err(_) => return false,
            };
            if !self.reaped
                && matches!(
                    platform::group_closed(self.receipt.pgid, &[&self.receipt.identity]),
                    Observed::Verified(..) | Observed::Absent(_)
                )
                && matches!(spawn::reap(self.receipt.identity.pid), Ok(Some(_)))
            {
                self.reaped = true;
            }
            if self.reaped && capture.stdout.is_none() && capture.stderr.is_none() {
                capture.finish();
                return true;
            }
            if cx.operation().check("probe closure").is_err() {
                return false;
            }
            // A reaped writer can leave several pipe reads buffered. Drain
            // that finite output to EOF before flushing the redactor.
            if !moved {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}
impl Drop for Pin {
    fn drop(&mut self) {
        if !self.reaped {
            platform::owner_signal(&self.receipt, libc::SIGKILL);
            // Never block beyond a context budget on a D-state process. The
            // guard still has the registration and generation evidence.
            let _ = spawn::reap(self.receipt.identity.pid);
        }
    }
}
struct Capture {
    stdout: Option<OwnedFd>,
    stderr: Option<OwnedFd>,
    redactor: Redactor,
    bytes: Vec<u8>,
    max: usize,
    truncated: bool,
}
impl Capture {
    fn drain(&mut self) -> io::Result<bool> {
        let mut moved = false;
        // One bounded read from each stream per round prevents an infinite
        // writer from starving the process deadline or the other stream.
        for stream in [&mut self.stdout, &mut self.stderr] {
            let Some(fd) = stream else {
                continue;
            };
            let mut chunk = [0; 4096];
            // SAFETY: live nonblocking descriptor and initialized writable buf.
            let count =
                unsafe { libc::read(fd.as_raw_fd(), chunk.as_mut_ptr().cast(), chunk.len()) };
            if count == 0 {
                moved = true;
                *stream = None;
            } else if count > 0 {
                moved = true;
                let redacted = self.redactor.push(&chunk[..count as usize], false);
                let keep = redacted
                    .len()
                    .min(self.max.saturating_sub(self.bytes.len()));
                self.bytes.extend_from_slice(&redacted[..keep]);
                self.truncated |= keep < redacted.len();
            } else if !matches!(
                io::Error::last_os_error().kind(),
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
            ) {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(moved)
    }
    fn finish(&mut self) {
        let rest = self.redactor.push(&[], true);
        let keep = rest.len().min(self.max.saturating_sub(self.bytes.len()));
        self.bytes.extend_from_slice(&rest[..keep]);
        self.truncated |= keep < rest.len() || self.stdout.is_some() || self.stderr.is_some();
    }
}
